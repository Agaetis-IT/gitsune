//! Targets of the editing milestones (see SPEC §12.6): `git status` and staging on a large
//! working tree, and never more than one `git status` at a time.

use std::fs;
use std::sync::atomic::Ordering;
use std::time::Instant;

use tempfile::TempDir;

use super::ops::{OpCtx, stage_paths};
use super::ops_tests::repo_with;
use super::speed::{enable_untracked_cache, status_hint};
use super::status_tests::write;
use super::store::RepoStores;
use super::tests::git;

#[test]
fn many_simultaneous_status_requests_run_one_at_a_time() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let ctx = OpCtx {
                    workdir: tmp.path(),
                    stores: &stores,
                };
                for _ in 0..5 {
                    ctx.status().unwrap();
                }
            });
        }
    });
    assert_eq!(stores.status_peak.load(Ordering::SeqCst), 1);
}

/// A repository of `files` small files spread over folders, committed.
fn big_working_tree(files: usize) -> TempDir {
    let tmp = repo_with(&[("seed.txt", "seed\n")]);
    let dir = tmp.path();
    for index in 0..files {
        let folder = dir.join(format!("d{:04}/s{:02}", index / 100, (index / 10) % 10));
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join(format!("f{index}.txt")),
            format!("file {index}\n"),
        )
        .unwrap();
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "many files"]);
    tmp
}

/// `cargo test --release perf_ -- --ignored --nocapture`: prints the timings that SPEC §12.6
/// compares with its targets (status under 300 ms, staging under 150 ms).
#[test]
#[ignore = "builds a repository of 100,000 files: slow, run on demand"]
fn perf_status_and_staging_on_100k_files() {
    let tmp = big_working_tree(100_000);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = OpCtx {
        workdir: dir,
        stores: &stores,
    };
    let time = |label: &str, run: &mut dyn FnMut()| {
        let started = Instant::now();
        run();
        println!("{label}: {} ms", started.elapsed().as_millis());
    };

    // The first status refreshes the index stat information; the ones after are what the
    // watcher triggers.
    time("status (first, cold)", &mut || {
        ctx.status().unwrap();
    });
    time("status (clean, warm)", &mut || {
        ctx.status().unwrap();
    });

    write(dir, "d0001/s00/f100.txt", "edited\n");
    time("status (1 modified file)", &mut || {
        let status = ctx.status().unwrap();
        assert_eq!(status.unstaged.len(), 1);
    });

    for index in 0..1000 {
        write(dir, &format!("new/n{index}.txt"), "new\n");
    }
    println!("hint before: {:?}", status_hint(&ctx).unwrap());
    time("status (1 modified + 1000 untracked files)", &mut || {
        let status = ctx.status().unwrap();
        // The new files are reported as one untracked folder.
        assert_eq!(status.unstaged.len(), 2);
    });

    let hint = enable_untracked_cache(&ctx).unwrap();
    println!("hint after enabling: {hint:?}");
    time("status (untracked cache on, 1 staged)", &mut || {
        ctx.status().unwrap();
    });
    write(dir, "d0002/s00/f200.txt", "edited again\n");
    time(
        "status (untracked cache on, 1 staged + 1 modified)",
        &mut || {
            ctx.status().unwrap();
        },
    );
    time(
        "stage one file + status (what the interface waits for)",
        &mut || {
            let status = stage_paths(&ctx, &["d0001/s00/f100.txt".to_string()]).unwrap();
            assert_eq!(status.staged.len(), 1);
        },
    );
}
