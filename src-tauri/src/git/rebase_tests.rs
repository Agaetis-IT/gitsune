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

// ---------- branches shaped like a team repository (pull request merges) ----------

/// develop with merged pull requests; `feature` branched from an older develop and merged
/// develop into itself once, as people do.
fn team_repo() -> TempDir {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["branch", "-m", "develop"]);
    for n in 1..=3 {
        git(dir, &["switch", "-q", "-c", &format!("pr/{n}")]);
        commit_file(dir, &format!("pr{n}.txt"), "x\n", &format!("PR {n} work"));
        git(dir, &["switch", "-q", "develop"]);
        git(
            dir,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                &format!("Merged PR {n}"),
                &format!("pr/{n}"),
            ],
        );
    }
    git(dir, &["switch", "-q", "-c", "feature", "HEAD~3"]);
    commit_file(dir, "feature1.txt", "f1\n", "feature one");
    git(
        dir,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge develop into feature",
            "develop",
        ],
    );
    commit_file(dir, "feature2.txt", "f2\n", "feature two");
    git(dir, &["switch", "-q", "develop"]);
    commit_file(dir, "later.txt", "l\n", "later on develop");
    git(dir, &["switch", "-q", "feature"]);
    tmp
}

#[test]
fn rebases_a_branch_that_contains_merges_onto_its_base_branch() {
    let tmp = team_repo();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let plan = rebase_plan(&ctx, "develop").unwrap();
    assert_eq!(plan.merges, 1);
    let result = rebase(&ctx, "develop", None, false).unwrap();
    assert_eq!(
        result.outcome,
        RebaseOutcome::Done,
        "{:?}",
        result.status.operation
    );
    assert!(dir.join("later.txt").exists());
    assert_eq!(
        subjects(dir, "develop..HEAD"),
        vec!["feature one", "feature two"]
    );
}

#[test]
fn interactive_rebase_works_on_such_a_branch_too() {
    let tmp = team_repo();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = plan_items(&tmp, &stores, "develop");
    let items = vec![
        item(TodoAction::Pick, &ids[0], None),
        item(TodoAction::Reword, &ids[1], Some("feature two, reworded")),
    ];
    let result = rebase(&ctx, "develop", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    assert_eq!(
        subjects(dir, "develop..HEAD"),
        vec!["feature one", "feature two, reworded"]
    );
}

#[test]
fn rebasing_onto_a_remote_tracking_branch_and_onto_a_commit_id_works() {
    let tmp = team_repo();
    let dir = tmp.path();
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "develop"]);
    git(
        dir,
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    git(dir, &["push", "-q", "origin", "develop"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = rebase(&ctx, "origin/develop", None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    undo_replay(&ctx, &result.before, &result.after).unwrap();

    let id = text(dir, &["rev-parse", "develop"]);
    let result = rebase(&ctx, &id, None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
}

#[test]
fn a_branch_whose_upstream_was_deleted_on_the_remote_can_still_be_rebased() {
    let (tmp, _ids) = diverged();
    let dir = tmp.path();
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        dir,
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    git(dir, &["push", "-q", "-u", "origin", "topic"]);
    // The pull request was merged and the branch deleted on the server; a fetch with prune
    // removes `origin/topic` but the local branch still names it as its upstream.
    git(remote.path(), &["branch", "-q", "-D", "topic"]);
    git(dir, &["fetch", "-q", "--prune", "origin"]);
    assert!(
        text(dir, &["status", "--porcelain=v2", "--branch"])
            .contains("branch.upstream origin/topic")
    );

    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let plan = rebase_plan(&ctx, "main").unwrap();
    assert_eq!(plan.commits.len(), 3);
    assert_eq!(
        plan.pushed, 0,
        "nothing can be said to be on a remote that is gone"
    );
    let result = rebase(&ctx, "main", None, false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
}

// ---------- messages, and stops that are not conflicts ----------

#[test]
fn a_reword_survives_a_path_with_spaces_and_several_lines() {
    let tmp = TempDir::new_in(std::env::temp_dir()).unwrap();
    let dir = tmp.path().join("a repo with spaces");
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Alice Fakename"]);
    git(&dir, &["config", "user.email", "alice@example.invalid"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    commit_file(&dir, "a.txt", "base\n", "base");
    git(&dir, &["switch", "-q", "-c", "topic"]);
    let one = commit_file(&dir, "one.txt", "1\n", "topic one");
    git(&dir, &["switch", "-q", "main"]);
    commit_file(&dir, "m.txt", "m\n", "main work");
    git(&dir, &["switch", "-q", "topic"]);
    let stores = RepoStores::default();
    let ctx = OpCtx {
        workdir: &dir,
        stores: &stores,
    };
    let items = vec![item(
        TodoAction::Reword,
        &one,
        Some("New subject\n\nA body line\nand another"),
    )];
    let result = rebase(&ctx, "main", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Done);
    assert_eq!(
        text(&dir, &["log", "-1", "--format=%B"]),
        "New subject\n\nA body line\nand another"
    );
}

#[test]
fn two_commits_with_the_same_first_line_each_get_their_own_message() {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    let first = commit_file(dir, "one.txt", "1\n", "wip");
    let second = commit_file(dir, "two.txt", "2\n", "wip");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "m.txt", "m\n", "main work");
    git(dir, &["switch", "-q", "topic"]);
    let stores = RepoStores::default();
    let items = vec![
        item(TodoAction::Reword, &first, Some("first, named")),
        item(TodoAction::Reword, &second, Some("second, named")),
    ];
    rebase(&ctx(&tmp, &stores), "main", Some(&items), false).unwrap();
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["first, named", "second, named"]
    );
}

#[test]
fn a_hook_that_refuses_the_reworded_commit_is_reported_and_continue_tries_again() {
    let (tmp, _ids) = diverged();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = plan_items(&tmp, &stores, "main");
    // A commit-msg hook that refuses until a file exists.
    let hook = dir.join(".git/hooks/commit-msg");
    fs::create_dir_all(dir.join(".git/hooks")).unwrap();
    fs::write(
        &hook,
        "#!/bin/sh\nif [ ! -f \"$(dirname \"$0\")/allow\" ]; then echo 'hook: not yet' >&2; exit 1; fi\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let items = vec![
        item(TodoAction::Reword, &ids[0], Some("Reworded one")),
        item(TodoAction::Pick, &ids[1], None),
        item(TodoAction::Pick, &ids[2], None),
    ];

    // The hook refuses: the app says what git said, not "conflicts", and the rebase waits.
    let error = rebase(&ctx, "main", Some(&items), false).unwrap_err();
    assert!(error.message.contains("not yet"), "{}", error.message);
    let status = ctx.status().unwrap();
    assert_eq!(
        status.operation.as_ref().unwrap().kind,
        OperationKind::Rebase
    );
    assert!(status.conflicted.is_empty());

    // The cause is fixed; Continue goes on and the chosen message is still used.
    fs::write(dir.join(".git/hooks/allow"), "").unwrap();
    let status = continue_operation(&ctx).unwrap();
    assert!(status.operation.is_none(), "{:?}", status.operation);
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["Reworded one", "topic two", "topic three"]
    );
}

#[test]
fn a_squash_message_survives_a_stop_on_a_conflict_in_the_group() {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    let first = commit_file(dir, "b.txt", "b\n", "topic adds b");
    let second = commit_file(dir, "a.txt", "topic version\n", "topic edits a");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main version\n", "main edits a");
    git(dir, &["switch", "-q", "topic"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let items = vec![
        item(TodoAction::Pick, &first, None),
        item(
            TodoAction::Squash,
            &second,
            Some("Both changes, one commit"),
        ),
    ];
    let result = rebase(&ctx, "main", Some(&items), false).unwrap();
    assert_eq!(result.outcome, RebaseOutcome::Conflicts);
    resolve_file(&ctx, "a.txt", "resolved\n").unwrap();
    let status = continue_operation(&ctx).unwrap();
    assert!(status.operation.is_none());
    assert_eq!(
        subjects(dir, "main..HEAD"),
        vec!["Both changes, one commit"]
    );
}
