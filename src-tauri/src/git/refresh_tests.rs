//! Rebuilding a graph after the repository changed, and finding commits again afterwards.

use std::collections::HashSet;
use std::path::Path;

use tempfile::TempDir;

use super::graph_page::{GraphCache, build_graph_cache};
use super::tests::{commit, git};
use super::{
    RepoStores, build_graph_cache_with, graph_meta, graph_page, graph_search, locate_commits,
    open_repository,
};

fn none() -> HashSet<String> {
    HashSet::new()
}

/// Everything a frontend can observe of a graph, except timings.
fn observable(repo: &gix::ThreadSafeRepository, cache: &GraphCache) -> serde_json::Value {
    let mut meta = serde_json::to_value(graph_meta(cache)).unwrap();
    meta["loadMs"] = 0.into();
    meta["layoutMs"] = 0.into();
    let page = graph_page(repo, cache, 0, 1000).unwrap();
    serde_json::json!({ "meta": meta, "rows": page.rows })
}

fn rev(dir: &Path, name: &str) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", name])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// main with a merged feature branch (deleted afterwards), like the layout tests.
fn merged() -> TempDir {
    super::tests::merged_repo()
}

#[test]
fn a_rebuild_after_changes_equals_a_build_from_scratch() {
    let tmp = merged();
    let dir = tmp.path();
    let (repo, _) = open_repository(dir).unwrap();
    let stores = RepoStores::default();
    build_graph_cache_with(&repo, &none(), &stores).unwrap();

    // New commits on main, a new branch with its own commit, and a tag.
    commit(dir, "third");
    git(dir, &["checkout", "-q", "-b", "topic"]);
    commit(dir, "topic work");
    git(dir, &["checkout", "-q", "main"]);
    commit(dir, "fourth");
    git(
        dir,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'topic'",
            "topic",
        ],
    );
    git(dir, &["tag", "v1"]);

    let rebuilt = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    let scratch = build_graph_cache(&repo, &none()).unwrap();
    assert_eq!(observable(&repo, &rebuilt), observable(&repo, &scratch));
    assert_eq!(graph_meta(&rebuilt).total, 8);

    // A fresh open of the same repository gives the same graph too.
    let (reopened, _) = open_repository(dir).unwrap();
    let fresh = build_graph_cache(&reopened, &none()).unwrap();
    assert_eq!(observable(&repo, &rebuilt), observable(&reopened, &fresh));
}

#[test]
fn a_rebuild_reuses_the_commits_it_already_read() {
    let tmp = merged();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let stores = RepoStores::default();
    build_graph_cache_with(&repo, &none(), &stores).unwrap();
    assert_eq!(stores.commits.lock().unwrap().commits.len(), 4);

    // Tamper with the remembered subject of the merge: only a build that reuses the store sees it.
    {
        let mut commits = stores.commits.lock().unwrap();
        let merge = commits
            .commits
            .values_mut()
            .find(|c| c.parents.len() == 2)
            .unwrap();
        merge.subject = "Merge branch 'from-the-store'".to_string();
    }
    let reused = graph_meta(&build_graph_cache_with(&repo, &none(), &stores).unwrap());
    assert!(
        reused
            .lanes
            .iter()
            .any(|l| l.name.as_deref() == Some("from-the-store"))
    );
    let scratch = graph_meta(&build_graph_cache(&repo, &none()).unwrap());
    assert!(
        scratch
            .lanes
            .iter()
            .any(|l| l.name.as_deref() == Some("feature/demo"))
    );
}

#[test]
fn hiding_refs_between_builds_keeps_the_store_complete_and_correct() {
    let tmp = super::tests::merged_repo_with_unmerged_branch();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let stores = RepoStores::default();
    let hidden: HashSet<String> = ["refs/heads/wip".to_string()].into();

    let without = build_graph_cache_with(&repo, &hidden, &stores).unwrap();
    assert_eq!(graph_meta(&without).total, 2);
    let with_all = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    assert_eq!(graph_meta(&with_all).total, 3);
    let again = build_graph_cache_with(&repo, &hidden, &stores).unwrap();
    assert_eq!(observable(&repo, &again), observable(&repo, &without));
}

#[test]
fn the_same_repository_always_gives_the_same_order() {
    // Commits made within the same second tie on time: ids decide, so the order never changes.
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, "root");
    for branch in ["a", "b", "c", "d"] {
        git(dir, &["checkout", "-q", "-b", branch, "main"]);
        commit(dir, &format!("work on {branch}"));
    }
    let (repo, _) = open_repository(dir).unwrap();
    let first = build_graph_cache(&repo, &none()).unwrap();
    for _ in 0..3 {
        let again = build_graph_cache(&repo, &none()).unwrap();
        assert_eq!(observable(&repo, &first), observable(&repo, &again));
    }
}

#[test]
fn commits_are_found_again_after_a_rebuild() {
    let tmp = merged();
    let dir = tmp.path();
    let (repo, _) = open_repository(dir).unwrap();
    let stores = RepoStores::default();
    let before = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    let main_before = rev(dir, "HEAD");
    let old_row = locate_commits(&before, std::slice::from_ref(&main_before))[0].unwrap();
    assert_eq!(old_row, 0, "the merge is the newest commit");

    commit(dir, "newer");
    let after = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    let found = locate_commits(&after, &[main_before.clone(), rev(dir, "HEAD")]);
    assert_eq!(
        found,
        vec![Some(1), Some(0)],
        "the old tip moved down by one row"
    );

    // Ids that are not in the graph, or not ids at all.
    let missing = locate_commits(&after, &["f".repeat(40), "nope".into(), String::new()]);
    assert_eq!(missing, vec![None, None, None]);

    // The rows found are the rows of the pages.
    let page = graph_page(&repo, &after, 0, 10).unwrap();
    assert_eq!(page.rows[1].id, main_before);
}

#[test]
fn the_search_index_follows_the_graph_and_reuses_its_entries() {
    let tmp = merged();
    let dir = tmp.path();
    let (repo, _) = open_repository(dir).unwrap();
    let stores = RepoStores::default();

    let before = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    assert_eq!(graph_search(&repo, &before, "newer", 10).total, 0);
    assert_eq!(stores.search.lock().unwrap().len(), 4);

    commit(dir, "newer commit");
    let after = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    let found = graph_search(&repo, &after, "newer", 10);
    assert_eq!((found.rows, found.total), (vec![0], 1));
    assert_eq!(
        stores.search.lock().unwrap().len(),
        5,
        "only the new commit was read"
    );
}

/// Refresh speed on a big repository (works on a copy, the original is left alone):
/// `SYNTH_REPO=/path cargo test --release refreshing_a_synthetic -- --ignored --nocapture`
#[test]
#[ignore = "needs SYNTH_REPO=<path to a synthetic repository>"]
fn refreshing_a_synthetic_repository() {
    use std::time::Instant;

    let source = std::env::var("SYNTH_REPO").expect("set SYNTH_REPO");
    let tmp = TempDir::new().unwrap();
    let copy = tmp.path().join("copy");
    let status = std::process::Command::new("cp")
        .args(["-R", &source])
        .arg(&copy)
        .status()
        .unwrap();
    assert!(status.success());
    // FEW_REFS=1 keeps ten feature branches only: a long history with a realistic number of refs.
    if std::env::var("FEW_REFS").is_ok() {
        let script = "git for-each-ref --format='delete %(refname)' refs/heads/feature refs/remotes/origin/feature | tail -n +11 | git update-ref --stdin";
        let status = std::process::Command::new("sh")
            .current_dir(&copy)
            .args(["-c", script])
            .status()
            .unwrap();
        assert!(status.success());
    }
    let (repo, _) = open_repository(&copy).unwrap();
    println!("{} refs", super::list_refs(&repo).unwrap().len());

    let stores = RepoStores::default();
    let started = Instant::now();
    let first = build_graph_cache_with(&repo, &none(), &stores).unwrap();
    println!("first build (reads every commit): {:?}", started.elapsed());
    graph_search(&repo, &first, "feature", 10); // fills the search store too

    git(&copy, &["checkout", "-q", "main"]);
    commit(&copy, "one more commit on main");
    for round in 1..=3 {
        let started = Instant::now();
        let cache = build_graph_cache_with(&repo, &none(), &stores).unwrap();
        println!(
            "refresh {round} after one commit: {:?} (load {} ms, layout {} ms)",
            started.elapsed(),
            graph_meta(&cache).load_ms,
            graph_meta(&cache).layout_ms
        );
        let started = Instant::now();
        let search = graph_search(&repo, &cache, "feature", 10);
        println!(
            "  search index rebuilt + query: {:?}, {} matches",
            started.elapsed(),
            search.total
        );
        let started = Instant::now();
        let tip = rev(&copy, "HEAD");
        let rows = locate_commits(&cache, &[tip]);
        println!("  locate: {:?} -> {rows:?}", started.elapsed());
        commit(&copy, "another commit");
    }
}
