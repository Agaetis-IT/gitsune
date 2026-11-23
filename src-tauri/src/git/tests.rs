//! Tests on tiny throwaway repositories built with the git CLI (invented data only).

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::refs::RefKind;
use super::{list_refs, load_graph, open_repository};
use crate::layout::{self, LaneKind};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=Alice Fakename",
            "-c",
            "user.email=alice@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .status()
        .expect("git must be installed");
    assert!(status.success(), "git {args:?} failed");
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}

/// main with two commits, a feature branch, a lightweight tag, an annotated tag
/// and a fake remote-tracking ref.
fn sample_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, "first");
    git(dir, &["branch", "feature/demo"]);
    commit(dir, "second");
    git(dir, &["tag", "v1.0.0"]);
    git(dir, &["tag", "-a", "v2.0.0", "-m", "annotated"]);
    git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    tmp
}

#[test]
fn opens_a_repository_and_reports_its_head_branch() {
    let tmp = sample_repo();
    let (_, info) = open_repository(tmp.path()).unwrap();
    assert_eq!(info.head_branch.as_deref(), Some("main"));
    assert!(!info.is_bare);
    assert_eq!(info.name, tmp.path().file_name().unwrap().to_string_lossy());
}

#[test]
fn opening_a_plain_directory_fails() {
    let tmp = TempDir::new().unwrap();
    let err = open_repository(tmp.path()).unwrap_err();
    assert!(err.contains("cannot open repository"), "{err}");
}

#[test]
fn lists_local_remote_and_tag_refs_sorted_by_kind_then_name() {
    let tmp = sample_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let refs = list_refs(&repo).unwrap();

    let summary: Vec<_> = refs.iter().map(|r| (r.kind, r.name.as_str())).collect();
    assert_eq!(
        summary,
        vec![
            (RefKind::Local, "feature/demo"),
            (RefKind::Local, "main"),
            (RefKind::Remote, "origin/main"),
            (RefKind::Tag, "v1.0.0"),
            (RefKind::Tag, "v2.0.0"),
        ]
    );
}

#[test]
fn marks_the_head_branch_and_peels_annotated_tags_to_commits() {
    let tmp = sample_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let refs = list_refs(&repo).unwrap();
    let by_name = |n: &str| refs.iter().find(|r| r.name == n).unwrap();

    assert!(by_name("main").is_head);
    assert!(!by_name("feature/demo").is_head);
    // Both tags point at the same commit as main once the annotated one is peeled.
    assert_eq!(by_name("v2.0.0").target, by_name("main").target);
    assert_eq!(by_name("v1.0.0").target, by_name("main").target);
    assert_ne!(by_name("feature/demo").target, by_name("main").target);
}

/// main: first -> second -> merge(feature/demo); feature/demo (deleted after the merge): side.
fn merged_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, "first");
    git(dir, &["checkout", "-q", "-b", "feature/demo"]);
    commit(dir, "side");
    git(dir, &["checkout", "-q", "main"]);
    commit(dir, "second");
    git(
        dir,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'feature/demo'",
            "feature/demo",
        ],
    );
    git(dir, &["branch", "-q", "-D", "feature/demo"]);
    tmp
}

#[test]
fn loads_the_graph_in_topological_order_and_names_the_merged_lane() {
    let tmp = merged_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let refs = list_refs(&repo).unwrap();
    let graph = load_graph(&repo, &refs).unwrap();

    assert_eq!(graph.ids.len(), 4);
    for (row, commit) in graph.commits.iter().enumerate() {
        assert!(
            commit.parents.iter().all(|&p| p > row),
            "row {row}: {commit:?}"
        );
    }
    // The merge commit comes first and its subject was read.
    assert_eq!(graph.commits[0].parents.len(), 2);
    assert_eq!(graph.commits[0].subject, "Merge branch 'feature/demo'");
    assert_eq!(graph.refs.len(), 1);
    assert_eq!(graph.refs[0].row, 0);

    let computed = layout::layout(&graph.commits, &graph.refs);
    layout::tests::validate(&graph.commits, &computed).unwrap();
    assert_eq!(computed.lanes[0].name.as_deref(), Some("main"));
    let merged = computed
        .lanes
        .iter()
        .find(|l| l.kind == LaneKind::Merged)
        .unwrap();
    assert_eq!(merged.name.as_deref(), Some("feature/demo"));
    assert_eq!(computed.max_cols, 2);
}

/// Full-size check: `SYNTH_REPO=/path/to/synthetic-repo cargo test --release -- --ignored --nocapture`
/// (generate the repository with `bun scripts/gen-synthetic-repo.ts`).
#[test]
#[ignore = "needs SYNTH_REPO=<path to a synthetic repository>"]
fn layout_of_a_synthetic_repository() {
    use std::time::Instant;

    let path = std::env::var("SYNTH_REPO").expect("set SYNTH_REPO");
    let (repo, _) = open_repository(Path::new(&path)).unwrap();
    let refs = list_refs(&repo).unwrap();

    let started = Instant::now();
    let graph = load_graph(&repo, &refs).unwrap();
    let load = started.elapsed();
    let started = Instant::now();
    let computed = layout::layout(&graph.commits, &graph.refs);
    let layout_time = started.elapsed();
    let started = Instant::now();
    layout::tests::validate(&graph.commits, &computed).unwrap();
    let validation = started.elapsed();

    let named = computed.lanes.iter().filter(|l| l.name.is_some()).count();
    let merged = computed
        .lanes
        .iter()
        .filter(|l| l.kind == LaneKind::Merged)
        .count();
    println!(
        "{} commits, {} refs, {} lanes ({named} named, {merged} merged), {} columns",
        graph.commits.len(),
        refs.len(),
        computed.lanes.len(),
        computed.max_cols
    );
    println!("load {load:?}, layout {layout_time:?}, validation {validation:?}");
}
