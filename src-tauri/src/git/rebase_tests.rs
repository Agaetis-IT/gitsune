//! Rebase, interactive rebase, skip and continue, on throwaway repositories.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::cli::ErrorKind;
use super::merge::{abort_operation, continue_operation, resolve_file, skip_operation};
use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
use super::rebase::*;
use super::rewrite::undo_replay;
use super::status::OperationKind;
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

fn subjects(dir: &Path, range: &str) -> Vec<String> {
    text(dir, &["log", "--reverse", "--format=%s", range])
        .lines()
        .map(str::to_string)
        .collect()
}

/// main has "base" then "main work"; `topic` forked from "base" has three commits.
fn diverged() -> (TempDir, Vec<String>) {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    let ids = vec![
        commit_file(dir, "one.txt", "1\n", "topic one"),
        commit_file(dir, "two.txt", "2\n", "topic two"),
        commit_file(dir, "three.txt", "3\n", "topic three"),
    ];
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "main.txt", "m\n", "main work");
    git(dir, &["switch", "-q", "topic"]);
    (tmp, ids)
}

fn item(action: TodoAction, id: &str, message: Option<&str>) -> TodoItem {
    TodoItem {
        action,
        id: id.to_string(),
        message: message.map(str::to_string),
    }
}

// ---------- the plan ----------

#[test]
fn the_plan_lists_the_commits_to_replay_oldest_first() {
    let (tmp, ids) = diverged();
    let stores = RepoStores::default();
    let plan = rebase_plan(&ctx(&tmp, &stores), "main").unwrap();
    assert_eq!(
        plan.commits
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(plan.commits[0].summary, "topic one");
    assert_eq!(plan.merges, 0);
    assert_eq!(plan.pushed, 0);
}

#[test]
fn the_plan_counts_commits_already_on_the_upstream_and_merges() {
    let (tmp, _ids) = diverged();
    let dir = tmp.path();
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        dir,
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    git(dir, &["push", "-q", "-u", "origin", "topic"]);
    commit_file(dir, "local.txt", "l\n", "only local");
    let stores = RepoStores::default();
    let plan = rebase_plan(&ctx(&tmp, &stores), "main").unwrap();
    assert_eq!(plan.commits.len(), 4);
    assert_eq!(plan.pushed, 3, "three of the four were pushed");

    git(
        dir,
        &["merge", "-q", "--no-ff", "-m", "merge main in", "main"],
    );
    let plan = rebase_plan(&ctx(&tmp, &stores), "main").unwrap();
    assert_eq!(plan.merges, 1, "the merge commit is counted, not listed");
    assert_eq!(plan.commits.len(), 4);
}

#[test]
fn a_target_that_looks_like_an_option_is_refused() {
    let (tmp, _) = diverged();
    let stores = RepoStores::default();
    assert!(rebase_plan(&ctx(&tmp, &stores), "--abort").is_err());
    assert!(rebase(&ctx(&tmp, &stores), "--abort", None, false).is_err());
    assert!(stores.activity.list().is_empty());
}

// ---------- plain rebase ----------

#[test]
fn rebases_the_branch_onto_another_and_can_be_undone() {
    let (tmp, _ids) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let before = text(dir, &["rev-parse", "HEAD"]);

    let result = rebase(&ctx, "main", None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    assert_eq!(result.before, before);
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["topic one", "topic two", "topic three"]
    );
    assert!(dir.join("main.txt").exists(), "built on top of main");
    assert_eq!(
        text(dir, &["merge-base", "HEAD", "main"]),
        text(dir, &["rev-parse", "main"])
    );

    undo_replay(&ctx, &result.before, &result.after).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), before);
    assert!(!dir.join("main.txt").exists());
}

#[test]
fn rebasing_onto_what_is_already_underneath_changes_nothing() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let result = rebase(&ctx(&tmp, &stores), "main", None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::UpToDate);
    assert_eq!(result.before, result.after);
}

#[test]
fn a_dirty_tree_is_refused_unless_it_is_auto_stashed() {
    let (tmp, _ids) = diverged();
    let dir = tmp.path();
    write(dir, "one.txt", "edited\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let error = rebase(&ctx, "main", None, false).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::WouldOverwriteChanges,
        "{}",
        error.message
    );
    assert!(!dir.join(".git/rebase-merge").exists());

    let result = rebase(&ctx, "main", None, true).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    assert_eq!(fs::read_to_string(dir.join("one.txt")).unwrap(), "edited\n");
}

#[test]
fn a_detached_head_and_a_busy_repository_are_refused() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    git(dir, &["switch", "-q", "--detach"]);
    assert!(
        rebase(&ctx(&tmp, &stores), "main", None, false)
            .unwrap_err()
            .message
            .contains("detached")
    );
    git(dir, &["switch", "-q", "topic"]);
    // A cherry-pick waiting in a terminal.
    fs::write(
        dir.join(".git/CHERRY_PICK_HEAD"),
        text(dir, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    let error = rebase(&ctx(&tmp, &stores), "main", None, false).unwrap_err();
    assert!(error.message.contains("waiting"), "{}", error.message);
}

#[test]
fn a_rebase_is_refused_in_read_only_mode() {
    let (tmp, _) = diverged();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    assert_eq!(
        rebase(&ctx(&tmp, &stores), "main", None, false)
            .unwrap_err()
            .kind,
        ErrorKind::ReadOnly
    );
}

// ---------- conflicts ----------

/// main and topic both edit a.txt; topic has two commits, the first conflicts.
fn conflicting() -> TempDir {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    commit_file(dir, "a.txt", "topic version\n", "topic edits a");
    commit_file(dir, "b.txt", "b\n", "topic adds b");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main version\n", "main edits a");
    git(dir, &["switch", "-q", "topic"]);
    tmp
}

#[test]
fn a_conflict_stops_the_rebase_and_continuing_finishes_it() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = rebase(&ctx, "main", None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Conflicts);
    let operation = result.status.operation.clone().unwrap();
    assert_eq!(operation.kind, OperationKind::Rebase);
    assert!(
        operation.message.starts_with("Commit 1 of 2"),
        "{}",
        operation.message
    );
    assert_eq!(result.status.conflicted[0].path, "a.txt");

    resolve_file(&ctx, "a.txt", "both versions\n").unwrap();
    let status = continue_operation(&ctx).unwrap();
    assert!(
        status.operation.is_none(),
        "the second commit applies cleanly"
    );
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["topic edits a", "topic adds b"]
    );
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "both versions\n"
    );
}

#[test]
fn skipping_leaves_the_commit_out() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    rebase(&ctx, "main", None, false).unwrap();

    let status = skip_operation(&ctx).unwrap();
    assert!(status.operation.is_none());
    assert_eq!(subjects(dir, "main..HEAD"), vec!["topic adds b"]);
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "main version\n"
    );
}

#[test]
fn aborting_puts_the_branch_back() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let before = text(dir, &["rev-parse", "HEAD"]);
    rebase(&ctx, "main", None, false).unwrap();

    let status = abort_operation(&ctx).unwrap();
    assert!(status.operation.is_none() && status.conflicted.is_empty());
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(text(dir, &["branch", "--show-current"]), "topic");
}

#[test]
fn only_a_rebase_or_a_cherry_pick_can_be_skipped() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    assert!(skip_operation(&ctx(&tmp, &stores)).is_err());
}

// ---------- interactive ----------

fn plan_items(tmp: &TempDir, stores: &RepoStores, onto: &str) -> Vec<String> {
    rebase_plan(&ctx(tmp, stores), onto)
        .unwrap()
        .commits
        .into_iter()
        .map(|c| c.id)
        .collect()
}

#[test]
fn reorders_drops_and_rewords() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ids = plan_items(&tmp, &stores, "main");
    let items = vec![
        item(TodoAction::Pick, &ids[2], None),
        item(TodoAction::Reword, &ids[0], Some("A better first line")),
        item(TodoAction::Drop, &ids[1], None),
    ];
    let result = rebase(&ctx(&tmp, &stores), "main", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["topic three", "A better first line"]
    );
    assert!(!dir.join("two.txt").exists(), "the dropped commit is gone");
    assert!(dir.join("one.txt").exists() && dir.join("three.txt").exists());
}

#[test]
fn squashes_and_fixes_up() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ids = plan_items(&tmp, &stores, "main");

    let items = vec![
        item(TodoAction::Pick, &ids[0], None),
        item(TodoAction::Fixup, &ids[1], None),
        item(TodoAction::Squash, &ids[2], Some("All three in one")),
    ];
    rebase(&ctx(&tmp, &stores), "main", Some(&items), false).unwrap();
    assert_eq!(subjects(dir, "main..HEAD"), vec!["All three in one"]);
    for file in ["one.txt", "two.txt", "three.txt"] {
        assert!(dir.join(file).exists(), "{file}");
    }
}

#[test]
fn a_squash_keeps_git_s_combined_message_when_none_is_given() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ids = plan_items(&tmp, &stores, "main");
    let items = vec![
        item(TodoAction::Pick, &ids[0], None),
        item(TodoAction::Squash, &ids[1], None),
        item(TodoAction::Pick, &ids[2], None),
    ];
    rebase(&ctx(&tmp, &stores), "main", Some(&items), false).unwrap();
    let body = text(dir, &["log", "--skip=1", "-1", "--format=%B"]);
    assert!(
        body.contains("topic one") && body.contains("topic two"),
        "{body}"
    );
}

#[test]
fn an_edit_stops_the_rebase_and_continuing_finishes_it() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = plan_items(&tmp, &stores, "main");
    let items = vec![
        item(TodoAction::Pick, &ids[0], None),
        item(TodoAction::Edit, &ids[1], None),
        item(TodoAction::Pick, &ids[2], None),
    ];
    let result = rebase(&ctx, "main", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Stopped);
    assert!(result.status.conflicted.is_empty());
    assert_eq!(result.status.operation.unwrap().kind, OperationKind::Rebase);
    assert_eq!(text(dir, &["log", "-1", "--format=%s"]), "topic two");

    // The user amends the commit, then continues.
    write(dir, "extra.txt", "extra\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--amend", "--no-edit"]);
    let status = continue_operation(&ctx).unwrap();
    assert!(status.operation.is_none());
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["topic one", "topic two", "topic three"]
    );
    assert!(dir.join("extra.txt").exists());
}

#[test]
fn a_reword_survives_a_conflict_in_the_middle() {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    let first = commit_file(dir, "a.txt", "topic version\n", "topic edits a");
    let second = commit_file(dir, "b.txt", "b\n", "topic adds b");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main version\n", "main edits a");
    git(dir, &["switch", "-q", "topic"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let items = vec![
        item(TodoAction::Pick, &first, None),
        item(
            TodoAction::Reword,
            &second,
            Some("Renamed after the conflict"),
        ),
    ];
    let result = rebase(&ctx, "main", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Conflicts);

    resolve_file(&ctx, "a.txt", "resolved\n").unwrap();
    continue_operation(&ctx).unwrap();
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["topic edits a", "Renamed after the conflict"]
    );
}

#[test]
fn a_list_that_does_not_match_the_commits_is_refused_before_git_runs() {
    let (tmp, _) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ids = plan_items(&tmp, &stores, "main");
    let head = text(dir, &["rev-parse", "HEAD"]);

    let missing = vec![item(TodoAction::Pick, &ids[0], None)];
    assert!(rebase(&ctx(&tmp, &stores), "main", Some(&missing), false).is_err());

    let foreign = vec![
        item(TodoAction::Pick, &ids[0], None),
        item(TodoAction::Pick, &ids[1], None),
        item(TodoAction::Pick, &"f".repeat(40), None),
    ];
    assert!(rebase(&ctx(&tmp, &stores), "main", Some(&foreign), false).is_err());

    let squash_first = vec![
        item(TodoAction::Squash, &ids[0], None),
        item(TodoAction::Pick, &ids[1], None),
        item(TodoAction::Pick, &ids[2], None),
    ];
    let error = rebase(&ctx(&tmp, &stores), "main", Some(&squash_first), false).unwrap_err();
    assert!(error.message.contains("first"), "{}", error.message);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), head);
    assert!(
        stores
            .activity
            .list()
            .iter()
            .all(|e| !e.command.contains("rebase"))
    );
}
