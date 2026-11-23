//! Tests on tiny throwaway repositories built with the git CLI (invented data only).

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::graph::load_graph;
use super::refs::RefKind;
use super::store::CommitStore;
use super::{list_refs, open_repository};
use crate::layout::{self, LaneKind};

pub(super) fn git(dir: &Path, args: &[&str]) {
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

pub(super) fn commit(dir: &Path, message: &str) {
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
pub(super) fn merged_repo() -> TempDir {
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
    let graph = load_graph(&repo, &refs, &mut CommitStore::default()).unwrap();

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
    let graph = load_graph(&repo, &refs, &mut CommitStore::default()).unwrap();
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

// ---------- paged graph ----------

use super::graph_page::build_graph_cache;
use super::{graph_meta, graph_page};

fn linear_repo(commits: usize) -> TempDir {
    let tmp = TempDir::new().unwrap();
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    for i in 0..commits {
        commit(tmp.path(), &format!("commit {i}"));
    }
    tmp
}

#[test]
fn pages_are_slices_of_one_consistent_layout() {
    let tmp = merged_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();

    let meta = graph_meta(&cache);
    assert_eq!(meta.total, 4);
    assert_eq!(meta.max_cols, 2);
    assert_eq!(meta.refs.len(), 1);
    assert!(
        meta.lanes
            .iter()
            .any(|l| l.name.as_deref() == Some("feature/demo"))
    );

    let first = graph_page(&repo, &cache, 0, 3).unwrap();
    assert_eq!(
        (first.start, first.rows.len(), first.next_cursor),
        (0, 3, Some(3))
    );
    let second = graph_page(&repo, &cache, 3, 3).unwrap();
    assert_eq!(
        (second.start, second.rows.len(), second.next_cursor),
        (3, 1, None)
    );

    let whole = graph_page(&repo, &cache, 0, 100).unwrap();
    let stitched: Vec<_> = first
        .rows
        .iter()
        .chain(&second.rows)
        .map(|r| r.id.clone())
        .collect();
    let all: Vec<_> = whole.rows.iter().map(|r| r.id.clone()).collect();
    assert_eq!(stitched, all);
    assert_eq!(whole.next_cursor, None);

    // Row 0 is the merge commit: details, parents and refs come with it.
    let merge = &whole.rows[0];
    assert_eq!(merge.summary, "Merge branch 'feature/demo'");
    assert_eq!(merge.author, "Alice Fakename");
    assert_eq!(merge.parents.len(), 2);
    assert!(merge.parents.iter().all(|p| all[1..].contains(p)));
    assert_eq!(merge.refs.len(), 1);
    assert!(merge.refs[0].is_head);
    assert_eq!(merge.refs[0].name, "main");
    assert_eq!(
        merge.edges.len(),
        1,
        "the merge forks one lane to the right"
    );
    assert!(whole.rows[1..].iter().all(|r| r.refs.is_empty()));
}

#[test]
fn cursor_walks_the_whole_graph_without_gaps_or_repeats() {
    let tmp = linear_repo(25);
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();

    let mut cursor = Some(0);
    let mut rows = Vec::new();
    while let Some(start) = cursor {
        let page = graph_page(&repo, &cache, start, 7).unwrap();
        assert!(page.rows.len() <= 7);
        cursor = page.next_cursor;
        rows.extend(page.rows);
    }
    assert_eq!(rows.len(), 25);
    assert!(rows.iter().enumerate().all(|(i, r)| r.row == i));
    // Newest first: the summary of row 0 is the last commit made.
    assert_eq!(rows[0].summary, "commit 24");
    assert_eq!(rows[24].summary, "commit 0");
    assert!(rows.iter().all(|r| r.col == 0 && r.edges.is_empty()));
}

#[test]
fn page_bounds_are_clamped() {
    let tmp = linear_repo(3);
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();

    // A zero limit still makes progress.
    assert_eq!(graph_page(&repo, &cache, 0, 0).unwrap().rows.len(), 1);
    // Past the end: empty page, no further cursor.
    let beyond = graph_page(&repo, &cache, 99, 10).unwrap();
    assert!(beyond.rows.is_empty() && beyond.next_cursor.is_none());
    assert_eq!((beyond.start, beyond.total), (3, 3));
}

#[test]
fn page_serializes_with_camel_case_keys() {
    let tmp = merged_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let page = graph_page(&repo, &cache, 0, 1).unwrap();

    let json = serde_json::to_value(&page).unwrap();
    assert_eq!(json["nextCursor"], 1);
    let row = &json["rows"][0];
    for key in [
        "row", "id", "parents", "summary", "author", "time", "col", "lane", "edges", "refs",
    ] {
        assert!(row.get(key).is_some(), "missing {key}");
    }
    assert_eq!(row["edges"][0]["bend"], "start");
    assert_eq!(row["refs"][0]["isHead"], true);

    let meta = serde_json::to_value(graph_meta(&cache)).unwrap();
    assert_eq!(meta["maxCols"], 2);
    assert!(meta["lanes"][0].get("firstRow").is_some());
}

/// Paging speed on a big repository:
/// `SYNTH_REPO=/path cargo test --release paging_a_synthetic -- --ignored --nocapture`
#[test]
#[ignore = "needs SYNTH_REPO=<path to a synthetic repository>"]
fn paging_a_synthetic_repository() {
    use std::time::Instant;

    let path = std::env::var("SYNTH_REPO").expect("set SYNTH_REPO");
    let (repo, _) = open_repository(Path::new(&path)).unwrap();

    let started = Instant::now();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let build = started.elapsed();
    let meta = graph_meta(&cache);
    println!(
        "build {build:?} (load {} ms, layout {} ms), {} rows, {} lanes, {} columns",
        meta.load_ms,
        meta.layout_ms,
        meta.total,
        meta.lanes.len(),
        meta.max_cols
    );
    let started = Instant::now();
    let json = serde_json::to_string(&meta).unwrap();
    println!(
        "meta serialized: {} KiB in {:?}",
        json.len() / 1024,
        started.elapsed()
    );

    for (label, start) in [
        ("top", 0),
        ("middle", meta.total / 2),
        ("bottom", meta.total - 200),
    ] {
        let started = Instant::now();
        let page = graph_page(&repo, &cache, start, 200).unwrap();
        let fetch = started.elapsed();
        let started = Instant::now();
        let json = serde_json::to_string(&page).unwrap();
        println!(
            "page {label:<6} 200 rows: {fetch:?} + serialize {:?}, {} KiB",
            started.elapsed(),
            json.len() / 1024
        );
    }

    let started = Instant::now();
    let mut cursor = Some(0);
    let mut seen = 0;
    while let Some(start) = cursor {
        let page = graph_page(&repo, &cache, start, 1000).unwrap();
        seen += page.rows.len();
        cursor = page.next_cursor;
    }
    println!(
        "walked all {seen} rows in pages of 1000: {:?}",
        started.elapsed()
    );
    assert_eq!(seen, meta.total);
}

/// Writes the whole graph of a (small) synthetic repository as JSON, for frontend demos and tests:
/// `SYNTH_REPO=/path FIXTURE_OUT=/path/graph.json cargo test dump_graph_fixture -- --ignored`
#[test]
#[ignore = "needs SYNTH_REPO and FIXTURE_OUT"]
fn dump_graph_fixture() {
    let path = std::env::var("SYNTH_REPO").expect("set SYNTH_REPO");
    let out = std::env::var("FIXTURE_OUT").expect("set FIXTURE_OUT");
    let (repo, _) = open_repository(Path::new(&path)).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let meta = graph_meta(&cache);
    let page = graph_page(&repo, &cache, 0, meta.total).unwrap();
    let json = serde_json::json!({ "meta": meta, "rows": page.rows });
    std::fs::write(&out, serde_json::to_string(&json).unwrap()).unwrap();
}

#[test]
fn hidden_refs_drop_the_commits_only_they_reach() {
    let tmp = merged_repo_with_unmerged_branch();
    let (repo, _) = open_repository(tmp.path()).unwrap();

    let all = build_graph_cache(&repo, &HashSet::new()).unwrap();
    assert_eq!(graph_meta(&all).total, 3);

    let hide =
        |names: &[&str]| -> HashSet<String> { names.iter().map(|n| n.to_string()).collect() };
    let without_wip = build_graph_cache(&repo, &hide(&["refs/heads/wip"])).unwrap();
    let meta = graph_meta(&without_wip);
    assert_eq!(meta.total, 2);
    assert!(meta.refs.iter().all(|r| r.info.name != "wip"));

    // Hiding a ref whose commit is also reachable from a visible ref keeps that commit.
    let without_tag = build_graph_cache(&repo, &hide(&["refs/tags/v1"])).unwrap();
    assert_eq!(graph_meta(&without_tag).total, 3);

    // Hiding everything gives an empty graph, and an empty page.
    let none = build_graph_cache(
        &repo,
        &hide(&["refs/heads/main", "refs/heads/wip", "refs/tags/v1"]),
    )
    .unwrap();
    let meta = graph_meta(&none);
    assert_eq!(
        (meta.total, meta.refs.len(), meta.lanes.len(), meta.max_cols),
        (0, 0, 0, 0)
    );
    let page = graph_page(&repo, &none, 0, 10).unwrap();
    assert!(page.rows.is_empty() && page.next_cursor.is_none());
}

#[test]
fn graph_refs_point_at_the_row_that_shows_them() {
    let tmp = merged_repo_with_unmerged_branch();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let meta = graph_meta(&cache);
    let page = graph_page(&repo, &cache, 0, 100).unwrap();

    assert_eq!(meta.refs.len(), 3);
    for reference in &meta.refs {
        let row = &page.rows[reference.row];
        assert!(
            row.refs
                .iter()
                .any(|r| r.full_name == reference.info.full_name)
        );
    }
    let json = serde_json::to_value(&meta).unwrap();
    assert!(json["refs"][0]["row"].as_u64().is_some());
    assert!(
        json["refs"][0].get("fullName").is_some(),
        "RefInfo fields are flattened"
    );
}

/// main: first -> second (tag v1); wip: first -> side (not merged).
pub(super) fn merged_repo_with_unmerged_branch() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, "first");
    git(dir, &["checkout", "-q", "-b", "wip"]);
    commit(dir, "side");
    git(dir, &["checkout", "-q", "main"]);
    commit(dir, "second");
    git(dir, &["tag", "v1"]);
    tmp
}
