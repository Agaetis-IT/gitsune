//! Search tests on throwaway repositories with invented authors and messages.

use std::collections::HashSet;
use std::path::Path;

use tempfile::TempDir;

use super::tests::{git, merged_repo_with_unmerged_branch};
use super::{build_graph_cache, graph_meta, graph_page, graph_search, open_repository};

fn commit_as(dir: &Path, author: &str, message: &str) {
    let who = format!(
        "{author} <{}@example.invalid>",
        author.split(' ').next().unwrap().to_lowercase()
    );
    git(
        dir,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "--author",
            &who,
            "-m",
            message,
        ],
    );
}

fn rev(dir: &Path, name: &str) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", name])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Rows, newest first: 0 "Fix login redirect" (Bob), 1 "Update docs" (Alice),
/// 2 "Fix parser crash" + body (Chloe), 3 "Add login form" (Bob).
fn sample() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit_as(dir, "Bob Placeholder", "Add login form");
    commit_as(
        dir,
        "Chloe Sample",
        "Fix parser crash\n\nHandles empty input in the Lexer",
    );
    commit_as(dir, "Alice Fakename", "Update docs");
    commit_as(dir, "Bob Placeholder", "Fix login redirect");
    tmp
}

fn run(tmp: &TempDir, query: &str, limit: usize) -> (Vec<usize>, usize) {
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let result = graph_search(&repo, &cache, query, limit);
    (result.rows, result.total)
}

#[test]
fn finds_commits_by_message_case_insensitively_newest_first() {
    let tmp = sample();
    assert_eq!(run(&tmp, "login", 100), (vec![0, 3], 2));
    assert_eq!(run(&tmp, "LOGIN", 100), (vec![0, 3], 2));
    assert_eq!(run(&tmp, "redirect", 100), (vec![0], 1));
}

#[test]
fn finds_commits_by_author_and_in_the_message_body() {
    let tmp = sample();
    assert_eq!(run(&tmp, "chloe", 100), (vec![2], 1));
    assert_eq!(run(&tmp, "placeholder", 100), (vec![0, 3], 2));
    assert_eq!(
        run(&tmp, "lexer", 100),
        (vec![2], 1),
        "the body of the message is searched"
    );
}

#[test]
fn every_word_must_match_somewhere_in_the_commit() {
    let tmp = sample();
    assert_eq!(run(&tmp, "fix login", 100), (vec![0], 1));
    assert_eq!(
        run(&tmp, "fix bob", 100),
        (vec![0], 1),
        "words may come from message and author"
    );
    assert_eq!(run(&tmp, "login docs", 100), (Vec::new(), 0));
}

#[test]
fn finds_a_commit_by_the_start_of_its_id() {
    let tmp = sample();
    let docs = rev(tmp.path(), "HEAD~1");
    assert_eq!(run(&tmp, &docs[..7], 100), (vec![1], 1));
    assert_eq!(run(&tmp, &docs[..10].to_uppercase(), 100), (vec![1], 1));
    assert_eq!(run(&tmp, &docs, 100), (vec![1], 1));
}

#[test]
fn blank_or_unmatched_queries_find_nothing() {
    let tmp = sample();
    assert_eq!(run(&tmp, "", 100), (Vec::new(), 0));
    assert_eq!(run(&tmp, "   \t ", 100), (Vec::new(), 0));
    assert_eq!(run(&tmp, "zzzz-nothing", 100), (Vec::new(), 0));
}

#[test]
fn the_limit_caps_the_rows_but_not_the_total() {
    let tmp = sample();
    assert_eq!(run(&tmp, "login", 1), (vec![0], 2));
    assert_eq!(
        run(&tmp, "login", 0),
        (vec![0], 2),
        "a zero limit still returns one row"
    );
}

#[test]
fn matches_are_the_rows_of_the_graph_pages() {
    let tmp = sample();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let result = graph_search(&repo, &cache, "login", 100);
    let page = graph_page(&repo, &cache, 0, 100).unwrap();
    for row in result.rows {
        assert!(page.rows[row].summary.to_lowercase().contains("login"));
    }
    // A second search reuses the index and gives the same answer.
    assert_eq!(graph_search(&repo, &cache, "login", 100).total, 2);
}

#[test]
fn search_only_covers_commits_of_the_visible_graph() {
    let tmp = merged_repo_with_unmerged_branch(); // "side" is only reachable from wip
    let (repo, _) = open_repository(tmp.path()).unwrap();

    let all = build_graph_cache(&repo, &HashSet::new()).unwrap();
    assert_eq!(graph_search(&repo, &all, "side", 10).total, 1);

    let hidden: HashSet<String> = ["refs/heads/wip".to_string()].into();
    let without = build_graph_cache(&repo, &hidden).unwrap();
    assert_eq!(graph_search(&repo, &without, "side", 10).total, 0);
}

#[test]
fn refs_know_their_lane_and_results_serialize_in_camel_case() {
    let tmp = merged_repo_with_unmerged_branch();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();
    let meta = graph_meta(&cache);
    assert!(!meta.refs.is_empty());
    for reference in &meta.refs {
        let lane = &meta.lanes[reference.lane as usize];
        assert!(lane.first_row <= reference.row && reference.row <= lane.last_row);
    }
    let json = serde_json::to_value(&meta).unwrap();
    assert!(json["refs"][0]["lane"].is_number());

    let found = serde_json::to_value(graph_search(&repo, &cache, "side", 10)).unwrap();
    assert_eq!(found["total"], 1);
    assert!(found["rows"].is_array());
}

/// Search speed on a big repository:
/// `SYNTH_REPO=/path cargo test --release searching_a_synthetic -- --ignored --nocapture`
#[test]
#[ignore = "needs SYNTH_REPO=<path to a synthetic repository>"]
fn searching_a_synthetic_repository() {
    use std::time::Instant;

    let path = std::env::var("SYNTH_REPO").expect("set SYNTH_REPO");
    let (repo, _) = open_repository(Path::new(&path)).unwrap();
    let cache = build_graph_cache(&repo, &HashSet::new()).unwrap();

    let started = Instant::now();
    let first = graph_search(&repo, &cache, "feature", 2000);
    println!(
        "first search (builds the index): {:?}, {} matches",
        started.elapsed(),
        first.total
    );
    for query in [
        "feature",
        "merge branch",
        "alice fakename",
        "update perf",
        "c66676a",
        "zzz",
    ] {
        let started = Instant::now();
        let result = graph_search(&repo, &cache, query, 2000);
        println!(
            "{query:<16} {:>9.2?}  {} matches",
            started.elapsed(),
            result.total
        );
    }
}
