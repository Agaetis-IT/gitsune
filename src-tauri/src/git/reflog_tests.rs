//! The history of positions and the undo built on it, on throwaway repositories.

use std::path::Path;

use tempfile::TempDir;

use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
use super::reflog::*;
use super::status_tests::write;
use super::store::RepoStores;
use super::tests::git;

fn text(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_out(dir, args))
        .trim()
        .to_string()
}

fn ctx<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    OpCtx {
        workdir: tmp.path(),
        stores,
    }
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) -> String {
    write(dir, name, content);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    text(dir, &["rev-parse", "HEAD"])
}

#[test]
fn lists_the_positions_newest_first() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let second = commit_file(dir, "b.txt", "two\n", "Add b");

    let entries = list_reflog(&ctx(&tmp, &stores)).unwrap();
    assert!(entries.len() >= 2);
    assert_eq!(entries[0].id, second);
    assert!(entries[0].current);
    assert!(entries[0].message.starts_with("commit: Add b"));
    assert!(!entries[1].current);
}

#[test]
fn undoes_a_commit_and_redoes_it() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let first = text(dir, &["rev-parse", "HEAD"]);
    let second = commit_file(dir, "b.txt", "two\n", "Add b");

    let undone = undo_last(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(undone.undone, "the commit");
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), first);

    undo_last(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(
        text(dir, &["rev-parse", "HEAD"]),
        second,
        "undoing twice is a redo"
    );
}

#[test]
fn undoes_a_checkout_by_switching_back() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let start = text(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(dir, &["switch", "-q", "-c", "feature"]);

    undo_last(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(text(dir, &["rev-parse", "--abbrev-ref", "HEAD"]), start);
}

#[test]
fn undoes_a_whole_rebase() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let base = text(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(dir, &["switch", "-q", "-c", "feature"]);
    commit_file(dir, "f1.txt", "1\n", "Feature one");
    commit_file(dir, "f2.txt", "2\n", "Feature two");
    let before = text(dir, &["rev-parse", "HEAD"]);
    git(dir, &["switch", "-q", &base]);
    commit_file(dir, "m.txt", "m\n", "Main moves on");
    git(dir, &["switch", "-q", "feature"]);
    git(dir, &["rebase", "-q", &base]);
    assert_ne!(text(dir, &["rev-parse", "HEAD"]), before);

    undo_last(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), before);
}

#[test]
fn restoring_refuses_to_overwrite_local_changes() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let first = text(dir, &["rev-parse", "HEAD"]);
    commit_file(dir, "a.txt", "two\n", "Change a");
    write(dir, "a.txt", "mine\n");

    assert!(restore_head(&ctx(&tmp, &stores), &first).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("a.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn restores_a_position_of_the_history() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let first = text(dir, &["rev-parse", "HEAD"]);
    commit_file(dir, "b.txt", "two\n", "Add b");

    restore_head(&ctx(&tmp, &stores), &first).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), first);
}

#[test]
fn lists_the_commits_a_branch_has_beyond_its_base() {
    use super::range::commits_between;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let base = text(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(dir, &["switch", "-q", "-c", "feat"]);
    commit_file(dir, "b.txt", "2\n", "Add b");
    commit_file(dir, "c.txt", "3\n", "Add c");

    let commits = commits_between(&ctx(&tmp, &stores), &base, "feat").unwrap();
    let summaries: Vec<_> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["Add b", "Add c"], "oldest first");
    assert!(commits_between(&ctx(&tmp, &stores), "-x", "feat").is_err());
    assert!(commits_between(&ctx(&tmp, &stores), "nope", "feat").is_err());
}

#[test]
fn lists_the_authors_of_every_branch() {
    use super::range::list_authors;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    git(dir, &["switch", "-q", "-c", "other"]);
    write(dir, "b.txt", "2\n");
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Grace Hopper",
            "-c",
            "user.email=g@example.test",
            "commit",
            "-q",
            "-m",
            "By Grace",
        ],
    );
    let authors = list_authors(&ctx(&tmp, &stores)).unwrap();
    assert!(
        authors
            .iter()
            .any(|a| a.name == "Grace Hopper" && a.count == 1)
    );
    assert!(
        authors.len() >= 2,
        "the initial author and Grace: {authors:?}"
    );
}

#[test]
fn blames_a_file_by_commit_and_by_working_tree() {
    use super::blame::blame_file;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    write(dir, "a.txt", "one\ntwo\n");
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Grace Hopper",
            "-c",
            "user.email=g@example.test",
            "commit",
            "-q",
            "-m",
            "Add two",
        ],
    );
    let head = text(dir, &["rev-parse", "HEAD"]);

    let blame = blame_file(&ctx(&tmp, &stores), Some(&head), "a.txt").unwrap();
    assert_eq!(blame.lines.len(), 2);
    assert_ne!(blame.lines[0].commit, blame.lines[1].commit);
    let second = &blame.commits[blame.lines[1].commit];
    assert_eq!(second.author, "Grace Hopper");
    assert_eq!(second.summary, "Add two");
    assert!(!second.uncommitted);

    // A line only in the working tree is not committed yet.
    write(dir, "a.txt", "one\ntwo\nthree\n");
    let working = blame_file(&ctx(&tmp, &stores), None, "a.txt").unwrap();
    assert_eq!(working.lines.len(), 3);
    assert!(working.commits[working.lines[2].commit].uncommitted);

    assert!(blame_file(&ctx(&tmp, &stores), Some("-x"), "a.txt").is_err());
    assert!(blame_file(&ctx(&tmp, &stores), None, "missing.txt").is_err());
}
