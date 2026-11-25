//! Benchmark `git2` vs `gix` on a repository (use the synthetic generator).
//! Usage: cargo run --release --example bench_git -- <repo-path>

use std::time::{Duration, Instant};

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = f();
    let elapsed = start.elapsed();
    println!("  {label:<38} {:>9.1} ms", elapsed.as_secs_f64() * 1000.0);
    (value, elapsed)
}

fn bench_git2(path: &str) {
    println!("git2");
    let (repo, _) = timed("open", || git2::Repository::open(path).unwrap());
    let (ref_count, _) = timed("list refs", || {
        repo.references()
            .unwrap()
            .filter_map(Result::ok)
            .count()
    });
    println!("  refs: {ref_count}");

    let (_, _) = timed("revwalk first 1000 (with summary)", || {
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME).unwrap();
        walk.push_glob("refs/*").unwrap();
        let mut n = 0;
        for oid in walk.take(1000) {
            let commit = repo.find_commit(oid.unwrap()).unwrap();
            let _ = (commit.summary().ok().flatten().map(str::len), commit.parent_count());
            n += 1;
        }
        n
    });

    let (oids, _) = timed("revwalk all (ids only)", || {
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME).unwrap();
        walk.push_glob("refs/*").unwrap();
        walk.filter_map(Result::ok).collect::<Vec<_>>()
    });
    println!("  commits: {}", oids.len());

    timed("load all commits (summary+parents)", || {
        let mut bytes = 0usize;
        for oid in &oids {
            let commit = repo.find_commit(*oid).unwrap();
            bytes += commit.summary().ok().flatten().map_or(0, str::len) + commit.parent_count();
        }
        bytes
    });

    timed("revwalk TIME-only first 1000", || {
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(git2::Sort::TIME).unwrap();
        walk.push_glob("refs/*").unwrap();
        walk.take(1000).count()
    });

    timed("FULL GRAPH one pass (id,parents,time)", || {
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(git2::Sort::TIME).unwrap();
        walk.push_glob("refs/*").unwrap();
        let mut rows = Vec::with_capacity(100_000);
        for oid in walk {
            let oid = oid.unwrap();
            let c = repo.find_commit(oid).unwrap();
            let parents: Vec<git2::Oid> = c.parent_ids().collect();
            rows.push((oid, parents, c.time().seconds()));
        }
        rows.len()
    });

    let sample = oids[oids.len() / 2];
    timed("diff of one commit (files + patch)", || {
        let commit = repo.find_commit(sample).unwrap();
        let parent_tree = commit.parent(0).ok().map(|p| p.tree().unwrap());
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit.tree().unwrap()), None)
            .unwrap();
        let mut patch_len = 0;
        for i in 0..diff.deltas().len() {
            if let Some(mut patch) = git2::Patch::from_diff(&diff, i).unwrap() {
                patch_len += patch.to_buf().unwrap().len();
            }
        }
        patch_len
    });
}

fn bench_gix(path: &str) {
    println!("gix");
    let (mut repo, _) = timed("open", || gix::open(path).unwrap());
    repo.object_cache_size_if_unset(256 * 1024 * 1024);
    let (tips, _) = timed("list refs", || {
        let platform = repo.references().unwrap();
        platform
            .all()
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|mut r| r.peel_to_id().ok().map(|id| id.detach()))
            .collect::<Vec<_>>()
    });
    println!("  refs: {}", tips.len());

    timed("revwalk first 1000 (with summary)", || {
        let walk = repo
            .rev_walk(tips.clone())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .unwrap();
        let mut n = 0;
        for info in walk.take(1000) {
            let info = info.unwrap();
            let commit = info.object().unwrap();
            let _ = (commit.message_raw_sloppy().len(), info.parent_ids().count());
            n += 1;
        }
        n
    });

    let (ids, _) = timed("revwalk all (ids only)", || {
        repo.rev_walk(tips.clone())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .unwrap()
            .filter_map(Result::ok)
            .map(|info| info.id)
            .collect::<Vec<_>>()
    });
    println!("  commits: {}", ids.len());

    timed("load all commits (summary+parents)", || {
        let mut bytes = 0usize;
        for id in &ids {
            let commit = repo.find_commit(*id).unwrap();
            bytes += commit.message_raw_sloppy().len() + commit.parent_ids().count();
        }
        bytes
    });

    timed("FULL GRAPH one pass (id,parents,time)", || {
        let walk = repo
            .rev_walk(tips.clone())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .unwrap();
        let mut rows = Vec::with_capacity(100_000);
        for info in walk {
            let info = info.unwrap();
            let parents: Vec<gix::ObjectId> = info.parent_ids().map(|p| p.detach()).collect();
            let time = info.commit_time();
            rows.push((info.id, parents, time));
        }
        rows.len()
    });

    let sample = ids[ids.len() / 2];
    timed("diff of one commit (file list only)", || {
        let commit = repo.find_commit(sample).unwrap();
        let tree = commit.tree().unwrap();
        let parent_tree = commit
            .parent_ids()
            .next()
            .map(|p| repo.find_commit(p).unwrap().tree().unwrap())
            .unwrap_or_else(|| repo.empty_tree());
        repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)
            .unwrap()
            .len()
    });
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: bench_git <repo-path>");
    // Run each library twice: the second pass shows warm-cache numbers.
    for pass in 1..=2 {
        println!("=== pass {pass} ===");
        bench_git2(&path);
        bench_gix(&path);
    }
}
