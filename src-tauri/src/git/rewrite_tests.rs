//! Stash, cherry-pick, revert and reset, on throwaway repositories.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::cli::ErrorKind;
use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
use super::rewrite::*;
use super::stash::*;
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

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

// ---------- stash ----------

#[test]
fn stashes_lists_applies_pops_and_drops() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    write(dir, "a.txt", "changed\n");
    write(dir, "new.txt", "untracked\n");
    let status = stash_push(&ctx, Some("my work"), true, false).unwrap();
    assert!(status.staged.is_empty() && status.unstaged.is_empty());
    assert!(
        !dir.join("new.txt").exists(),
        "untracked files are stashed too"
    );

    let list = list_stashes(&ctx).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].reference, "stash@{0}");
    assert!(list[0].message.contains("my work"), "{}", list[0].message);

    // Apply keeps the stash.
    stash_apply(&ctx, &list[0].reference, &list[0].id, false).unwrap();
    assert_eq!(read(dir, "a.txt"), "changed\n");
    assert_eq!(list_stashes(&ctx).unwrap().len(), 1);
    git(dir, &["checkout", "-q", "--", "a.txt"]);
    fs::remove_file(dir.join("new.txt")).unwrap();

    // Pop removes it.
    stash_pop(&ctx, &list[0].reference, &list[0].id, false).unwrap();
    assert_eq!(read(dir, "a.txt"), "changed\n");
    assert!(dir.join("new.txt").exists());
    assert!(list_stashes(&ctx).unwrap().is_empty());
}

#[test]
fn an_empty_tree_cannot_be_stashed_and_untracked_files_are_optional() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let error = stash_push(&ctx, None, true, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NothingToCommit);

    write(dir, "new.txt", "untracked\n");
    // Without --include-untracked there is nothing git wants to stash.
    assert!(stash_push(&ctx, None, false, false).is_err());
    assert!(dir.join("new.txt").exists());
}

#[test]
fn a_dropped_stash_can_be_brought_back() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    write(dir, "a.txt", "changed\n");
    stash_push(&ctx, Some("keep me"), false, false).unwrap();
    let entry = list_stashes(&ctx).unwrap().remove(0);

    let dropped = stash_drop(&ctx, &entry.reference, &entry.id).unwrap();
    assert!(list_stashes(&ctx).unwrap().is_empty());

    stash_restore(&ctx, &dropped.id, &dropped.message).unwrap();
    let again = list_stashes(&ctx).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, entry.id);
    assert!(again[0].message.contains("keep me"));
}

#[test]
fn a_stale_stash_position_is_refused_instead_of_acting_on_another_stash() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    write(dir, "a.txt", "first\n");
    stash_push(&ctx, Some("first"), false, false).unwrap();
    write(dir, "a.txt", "second\n");
    stash_push(&ctx, Some("second"), false, false).unwrap();
    let list = list_stashes(&ctx).unwrap();
    assert!(list[0].message.contains("second"));

    // The user looked at the list, then a terminal dropped the top stash.
    git(dir, &["stash", "drop", "-q"]);
    let error = stash_drop(&ctx, "stash@{0}", &list[0].id).unwrap_err();
    assert!(error.message.contains("changed"), "{}", error.message);
    assert_eq!(list_stashes(&ctx).unwrap().len(), 1, "nothing was dropped");
}

#[test]
fn a_stash_that_conflicts_is_kept_in_the_list() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    write(dir, "a.txt", "stashed\n");
    stash_push(&ctx, None, false, false).unwrap();
    commit_file(dir, "a.txt", "committed\n", "diverge");
    let entry = list_stashes(&ctx).unwrap().remove(0);

    let error = stash_pop(&ctx, &entry.reference, &entry.id, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Conflict, "{}", error.message);
    assert_eq!(
        list_stashes(&ctx).unwrap().len(),
        1,
        "the stash is still there"
    );
}

#[test]
fn stash_writes_are_refused_in_read_only_mode() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    write(tmp.path(), "a.txt", "changed\n");
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let error = stash_push(&ctx(&tmp, &stores), None, false, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::ReadOnly);
}

// ---------- cherry-pick and revert ----------

/// main (a.txt) and `side` with one extra commit adding side.txt.
fn with_side_commit() -> (TempDir, String) {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    let side = commit_file(dir, "side.txt", "side\n", "add side");
    git(dir, &["switch", "-q", "main"]);
    (tmp, side)
}

#[test]
fn cherry_picks_a_commit_and_can_take_it_back() {
    let (tmp, side) = with_side_commit();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    // Main moves on, so the copy has another parent: without it, a cherry-pick made in the same
    // second as the original would be the very same commit.
    commit_file(dir, "main.txt", "main\n", "main moves on");
    let before = text(dir, &["rev-parse", "HEAD"]);

    let result = cherry_pick(&ctx, &side).unwrap();
    assert_eq!(result.before, before);
    assert_eq!(read(dir, "side.txt"), "side\n");
    assert_eq!(text(dir, &["log", "-1", "--format=%s"]), "add side");
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), result.after);
    assert_ne!(result.after, side, "a new commit, not the same one");

    undo_replay(&ctx, &result.before, &result.after).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), before);
    assert!(!dir.join("side.txt").exists());
}

#[test]
fn a_cherry_pick_that_conflicts_is_cancelled_and_leaves_nothing_behind() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    let side = commit_file(dir, "a.txt", "side version\n", "side edits a");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main version\n", "main edits a");
    let stores = RepoStores::default();
    let before = text(dir, &["rev-parse", "HEAD"]);

    let error = cherry_pick(&ctx(&tmp, &stores), &side).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Conflict, "{}", error.message);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), before);
    assert!(!dir.join(".git/CHERRY_PICK_HEAD").exists());
    assert_eq!(read(dir, "a.txt"), "main version\n");
    assert!(text(dir, &["status", "--porcelain"]).is_empty());
}

#[test]
fn picking_a_commit_already_in_the_branch_says_so() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    let side = commit_file(dir, "side.txt", "side\n", "add side");
    git(dir, &["switch", "-q", "main"]);
    git(dir, &["merge", "-q", "--ff-only", "side"]);
    let stores = RepoStores::default();

    // Picking HEAD itself onto HEAD is empty.
    let error = cherry_pick(&ctx(&tmp, &stores), &side).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NothingToCommit, "{}", error.message);
    assert!(!dir.join(".git/CHERRY_PICK_HEAD").exists());
}

#[test]
fn a_cherry_pick_that_would_overwrite_local_changes_lists_the_files() {
    let (tmp, _side) = with_side_commit();
    let dir = tmp.path();
    git(dir, &["switch", "-q", "side"]);
    let edit = commit_file(dir, "a.txt", "side edit\n", "edit a on side");
    git(dir, &["switch", "-q", "main"]);
    write(dir, "a.txt", "unsaved\n");
    let stores = RepoStores::default();

    let error = cherry_pick(&ctx(&tmp, &stores), &edit).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::WouldOverwriteChanges,
        "{}",
        error.message
    );
    assert_eq!(error.files, vec!["a.txt".to_string()]);
    assert_eq!(read(dir, "a.txt"), "unsaved\n");
}

#[test]
fn reverts_a_commit_and_can_take_it_back() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let added = commit_file(dir, "b.txt", "b\n", "add b");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = revert_commit(&ctx, &added).unwrap();
    assert!(!dir.join("b.txt").exists());
    assert!(text(dir, &["log", "-1", "--format=%s"]).starts_with("Revert"));

    undo_replay(&ctx, &result.before, &result.after).unwrap();
    assert!(dir.join("b.txt").exists());
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), added);
}

#[test]
fn a_revert_that_conflicts_is_cancelled() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let first = commit_file(dir, "a.txt", "two\n", "two");
    commit_file(dir, "a.txt", "three\n", "three");
    let stores = RepoStores::default();
    let head = text(dir, &["rev-parse", "HEAD"]);

    let error = revert_commit(&ctx(&tmp, &stores), &first).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Conflict, "{}", error.message);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), head);
    assert!(!dir.join(".git/REVERT_HEAD").exists());
}

#[test]
fn reverting_a_merge_follows_its_first_parent() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    commit_file(dir, "side.txt", "side\n", "add side");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "main.txt", "main\n", "add main");
    git(dir, &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    let merge = text(dir, &["rev-parse", "HEAD"]);
    let stores = RepoStores::default();

    revert_commit(&ctx(&tmp, &stores), &merge).unwrap();
    assert!(!dir.join("side.txt").exists());
    assert!(dir.join("main.txt").exists());
}

#[test]
fn undoing_is_refused_once_head_moved() {
    let (tmp, side) = with_side_commit();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = cherry_pick(&ctx, &side).unwrap();
    commit_file(dir, "later.txt", "later\n", "later");

    let error = undo_replay(&ctx, &result.before, &result.after).unwrap_err();
    assert!(error.message.contains("moved"), "{}", error.message);
    assert!(dir.join("side.txt").exists());
}

#[test]
fn nothing_is_replayed_while_another_operation_is_waiting() {
    let (tmp, side) = with_side_commit();
    let dir = tmp.path();
    // A merge started in a terminal and stopped on a conflict.
    commit_file(dir, "a.txt", "main\n", "main edits a");
    git(dir, &["switch", "-q", "-c", "other", "HEAD~1"]);
    commit_file(dir, "a.txt", "other\n", "other edits a");
    git(dir, &["switch", "-q", "main"]);
    let _ = std::process::Command::new("git")
        .current_dir(dir)
        .args(["merge", "other"])
        .output();
    assert!(dir.join(".git/MERGE_HEAD").exists());
    let stores = RepoStores::default();

    let error = cherry_pick(&ctx(&tmp, &stores), &side).unwrap_err();
    assert!(error.message.contains("in progress"), "{}", error.message);
    assert!(
        dir.join(".git/MERGE_HEAD").exists(),
        "the user's merge was not touched"
    );
}

// ---------- reset ----------

/// Three commits (c1, c2, c3) on main.
fn three_commits() -> (TempDir, [String; 3]) {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let c1 = text(dir, &["rev-parse", "HEAD"]);
    let c2 = commit_file(dir, "a.txt", "two\n", "two");
    let c3 = commit_file(dir, "a.txt", "three\n", "three");
    (tmp, [c1, c2, c3])
}

#[test]
fn a_soft_reset_moves_the_branch_and_keeps_everything_staged() {
    let (tmp, [c1, _c2, c3]) = three_commits();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = reset(&ctx, &c1, ResetMode::Soft).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c1);
    assert_eq!(read(dir, "a.txt"), "three\n");
    assert_eq!(
        result.status.staged.len(),
        1,
        "the undone commits are staged"
    );
    assert!(result.snapshot.is_none());

    undo_reset(&ctx, &result.before, &result.after, None).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c3);
}

#[test]
fn a_mixed_reset_keeps_the_files_but_not_the_staging() {
    let (tmp, [c1, ..]) = three_commits();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let result = reset(&ctx(&tmp, &stores), &c1, ResetMode::Mixed).unwrap();
    assert_eq!(read(dir, "a.txt"), "three\n");
    assert!(result.status.staged.is_empty());
    assert_eq!(result.status.unstaged.len(), 1);
}

#[test]
fn a_hard_reset_discards_changes_but_undo_brings_them_back() {
    let (tmp, [c1, _c2, c3]) = three_commits();
    let dir = tmp.path();
    write(dir, "a.txt", "uncommitted work\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = reset(&ctx, &c1, ResetMode::Hard).unwrap();
    assert_eq!(read(dir, "a.txt"), "one\n");
    assert!(result.status.unstaged.is_empty());
    let snapshot = result.snapshot.clone().expect("changes were saved first");

    undo_reset(
        &ctx,
        &result.before,
        &result.after,
        Some(&snapshot.reference),
    )
    .unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c3);
    assert_eq!(read(dir, "a.txt"), "uncommitted work\n");
}

#[test]
fn a_hard_reset_on_a_clean_tree_takes_no_snapshot() {
    let (tmp, [c1, ..]) = three_commits();
    let stores = RepoStores::default();
    let result = reset(&ctx(&tmp, &stores), &c1, ResetMode::Hard).unwrap();
    assert!(result.snapshot.is_none());
}

#[test]
fn a_hard_reset_leaves_untracked_files_alone() {
    let (tmp, [c1, ..]) = three_commits();
    let dir = tmp.path();
    write(dir, "notes.txt", "mine\n");
    let stores = RepoStores::default();
    reset(&ctx(&tmp, &stores), &c1, ResetMode::Hard).unwrap();
    assert_eq!(read(dir, "notes.txt"), "mine\n");
}

#[test]
fn undoing_a_reset_is_refused_once_head_moved() {
    let (tmp, [c1, ..]) = three_commits();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = reset(&ctx, &c1, ResetMode::Hard).unwrap();
    commit_file(dir, "x.txt", "x\n", "after the reset");

    let error = undo_reset(&ctx, &result.before, &result.after, None).unwrap_err();
    assert!(error.message.contains("moved"), "{}", error.message);
}

#[test]
fn reset_refuses_a_target_that_is_not_a_commit_and_runs_nothing() {
    let (tmp, _) = three_commits();
    let stores = RepoStores::default();
    let error = reset(&ctx(&tmp, &stores), "no-such-thing", ResetMode::Hard).unwrap_err();
    assert!(error.message.contains("not a commit"), "{}", error.message);
    assert!(
        stores
            .activity
            .list()
            .iter()
            .all(|entry| !entry.command.contains("reset"))
    );
}

#[test]
fn rewrites_are_refused_in_read_only_mode() {
    let (tmp, [c1, ..]) = three_commits();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let ctx = ctx(&tmp, &stores);
    assert_eq!(
        reset(&ctx, &c1, ResetMode::Hard).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        cherry_pick(&ctx, &c1).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        revert_commit(&ctx, &c1).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
}

// ---------- reset of any branch ----------

#[test]
fn a_branch_that_is_not_checked_out_moves_and_the_files_stay() {
    let (tmp, [c1, c2, c3]) = three_commits();
    let dir = tmp.path();
    git(dir, &["branch", "side", &c2]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = reset_branch(&ctx, "side", &c1, ResetMode::Soft).unwrap();
    assert!(!result.current);
    assert_eq!(result.before.as_deref(), Some(c2.as_str()));
    assert_eq!(text(dir, &["rev-parse", "side"]), c1);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c3);
    assert_eq!(read(dir, "a.txt"), "three\n");

    undo_branch_reset(&ctx, "side", result.before.as_deref(), &result.after).unwrap();
    assert_eq!(text(dir, &["rev-parse", "side"]), c2);
}

#[test]
fn a_hard_reset_of_another_branch_only_moves_it_and_undo_brings_it_back() {
    let (tmp, [c1, c2, c3]) = three_commits();
    let dir = tmp.path();
    git(dir, &["branch", "side", &c2]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = reset_branch(&ctx, "side", &c1, ResetMode::Hard).unwrap();
    assert!(!result.current);
    assert_eq!(text(dir, &["rev-parse", "side"]), c1);
    // The checked-out branch and its files are not touched.
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c3);
    assert_eq!(read(dir, "a.txt"), "three\n");

    undo_branch_reset(&ctx, "side", result.before.as_deref(), &result.after).unwrap();
    assert_eq!(text(dir, &["rev-parse", "side"]), c2);
}

#[test]
fn a_hard_reset_of_a_branch_that_another_working_tree_has_is_refused() {
    let (tmp, [c1, c2, _c3]) = three_commits();
    let dir = tmp.path();
    git(dir, &["branch", "side", &c2]);
    let other = tmp.path().join("other-tree");
    git(
        dir,
        &["worktree", "add", "-q", other.to_str().unwrap(), "side"],
    );
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let error = reset_branch(&ctx, "side", &c1, ResetMode::Hard).unwrap_err();
    assert!(
        error.message.contains("another working tree"),
        "{}",
        error.message
    );
    assert_eq!(text(dir, &["rev-parse", "side"]), c2);
}

#[test]
fn resetting_the_checked_out_branch_goes_through_head() {
    let (tmp, [c1, ..]) = three_commits();
    let dir = tmp.path();
    let current = text(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let stores = RepoStores::default();

    let result = reset_branch(&ctx(&tmp, &stores), &current, &c1, ResetMode::Mixed).unwrap();
    assert!(result.current);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), c1);
}

#[test]
fn a_missing_branch_is_created_at_the_target_and_undo_deletes_it() {
    let (tmp, [c1, ..]) = three_commits();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = reset_branch(&ctx, "fresh", &c1, ResetMode::Soft).unwrap();
    assert_eq!(result.before, None);
    assert_eq!(text(dir, &["rev-parse", "fresh"]), c1);

    undo_branch_reset(&ctx, "fresh", None, &result.after).unwrap();
    assert!(!dir.join(".git/refs/heads/fresh").exists());
    assert!(!text(dir, &["branch", "--list", "fresh"]).contains("fresh"));
}

#[test]
fn undoing_a_branch_reset_is_refused_once_the_branch_moved() {
    let (tmp, [c1, c2, _c3]) = three_commits();
    let dir = tmp.path();
    git(dir, &["branch", "side", &c2]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = reset_branch(&ctx, "side", &c1, ResetMode::Soft).unwrap();
    git(dir, &["branch", "-f", "side", &c2]);

    let error =
        undo_branch_reset(&ctx, "side", result.before.as_deref(), &result.after).unwrap_err();
    assert!(error.message.contains("moved"), "{}", error.message);
}
