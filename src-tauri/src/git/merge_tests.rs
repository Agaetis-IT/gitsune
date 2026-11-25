//! Merging, conflicts and finishing or cancelling an operation, on throwaway repositories.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::cli::ErrorKind;
use super::merge::*;
use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
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

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

fn parents(dir: &Path, rev: &str) -> usize {
    text(dir, &["rev-list", "--parents", "-n1", rev])
        .split_whitespace()
        .count()
        - 1
}

/// main (a.txt) and `side`, one commit ahead of main (side.txt): a fast-forward is possible.
fn ahead() -> TempDir {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    commit_file(dir, "side.txt", "side\n", "add side");
    git(dir, &["switch", "-q", "main"]);
    tmp
}

/// main and `side` both edited line 1 of a.txt (a conflict) and each added a file.
fn conflicting() -> TempDir {
    let tmp = repo_with(&[("a.txt", "top\nmiddle\nbottom\n")]);
    let dir = tmp.path();
    git(dir, &["config", "merge.conflictStyle", "diff3"]);
    git(dir, &["switch", "-q", "-c", "side"]);
    commit_file(dir, "a.txt", "side top\nmiddle\nbottom\n", "side edits top");
    commit_file(dir, "side.txt", "side\n", "add side");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main top\nmiddle\nbottom\n", "main edits top");
    tmp
}

// ---------- merging ----------

#[test]
fn explains_what_a_merge_would_do() {
    let tmp = ahead();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let info = merge_info(&ctx, "side").unwrap();
    assert!(info.can_fast_forward && !info.up_to_date);
    assert_eq!(info.commits, 1);

    let same = merge_info(&ctx, "main").unwrap();
    assert!(same.up_to_date && !same.can_fast_forward);
    assert_eq!(same.commits, 0);

    commit_file(tmp.path(), "later.txt", "x\n", "main moves on");
    let diverged = merge_info(&ctx, "side").unwrap();
    assert!(!diverged.can_fast_forward && !diverged.up_to_date);
}

#[test]
fn fast_forwards_by_default_and_makes_a_merge_commit_when_asked() {
    let tmp = ahead();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = merge(&ctx, "side", MergeMode::Default, None).unwrap();
    assert_eq!(result.outcome, MergeOutcome::FastForward);
    assert_eq!(
        text(dir, &["rev-parse", "HEAD"]),
        text(dir, &["rev-parse", "side"])
    );

    // A second merge of the same branch has nothing to do.
    let again = merge(&ctx, "side", MergeMode::Default, None).unwrap();
    assert_eq!(again.outcome, MergeOutcome::UpToDate);

    let tmp = ahead();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let result = merge(
        &ctx_of(&tmp, &stores),
        "side",
        MergeMode::NoFastForward,
        Some("Bring side in"),
    )
    .unwrap();
    assert_eq!(result.outcome, MergeOutcome::Merged);
    assert_eq!(parents(dir, "HEAD"), 2);
    assert_eq!(text(dir, &["log", "-1", "--format=%s"]), "Bring side in");
}

fn ctx_of<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    ctx(tmp, stores)
}

#[test]
fn a_squash_merge_stages_the_changes_without_recording_a_merge() {
    let tmp = ahead();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let head = text(dir, &["rev-parse", "HEAD"]);

    let result = merge(&ctx(&tmp, &stores), "side", MergeMode::Squash, None).unwrap();
    assert_eq!(result.outcome, MergeOutcome::Squashed);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(result.status.staged.len(), 1);
    assert!(!dir.join(".git/MERGE_HEAD").exists());
}

#[test]
fn fast_forward_only_refuses_diverged_branches() {
    let tmp = conflicting();
    let stores = RepoStores::default();
    let error = merge(
        &ctx(&tmp, &stores),
        "side",
        MergeMode::FastForwardOnly,
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind, ErrorKind::DivergedBranches, "{}", error.message);
    assert!(!tmp.path().join(".git/MERGE_HEAD").exists());
}

#[test]
fn a_merge_can_be_undone_while_head_has_not_moved() {
    let tmp = ahead();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = merge(&ctx, "side", MergeMode::NoFastForward, None).unwrap();
    undo_replay(&ctx, &result.before, &result.after).unwrap();
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), result.before);
    assert!(!dir.join("side.txt").exists());
}

#[test]
fn a_merge_that_would_overwrite_local_changes_lists_the_files() {
    let tmp = ahead();
    let dir = tmp.path();
    git(dir, &["switch", "-q", "side"]);
    commit_file(dir, "a.txt", "side edit\n", "edit a on side");
    git(dir, &["switch", "-q", "main"]);
    write(dir, "a.txt", "unsaved\n");
    let stores = RepoStores::default();

    let error = merge(&ctx(&tmp, &stores), "side", MergeMode::Default, None).unwrap_err();
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
fn merge_arguments_that_look_like_options_are_refused() {
    let tmp = ahead();
    let stores = RepoStores::default();
    assert!(merge(&ctx(&tmp, &stores), "--abort", MergeMode::Default, None).is_err());
    assert!(stores.activity.list().is_empty());
}

#[test]
fn a_merge_is_refused_in_read_only_mode() {
    let tmp = ahead();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let error = merge(&ctx(&tmp, &stores), "side", MergeMode::Default, None).unwrap_err();
    assert_eq!(error.kind, ErrorKind::ReadOnly);
}

// ---------- conflicts ----------

#[test]
fn a_conflicting_merge_stays_in_progress_and_lists_the_files() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let head = text(dir, &["rev-parse", "HEAD"]);

    let result = merge(&ctx, "side", MergeMode::Default, None).unwrap();
    assert_eq!(result.outcome, MergeOutcome::Conflicts);
    assert_eq!(result.before, head);
    let operation = result.status.operation.expect("a merge is in progress");
    assert_eq!(operation.kind, OperationKind::Merge);
    assert!(
        operation.message.starts_with("Merge branch 'side'"),
        "{}",
        operation.message
    );
    assert_eq!(result.status.conflicted.len(), 1);
    assert_eq!(result.status.conflicted[0].path, "a.txt");
    assert_eq!(result.status.conflicted[0].code, "UU");

    // Nothing else can start meanwhile.
    let error = merge(&ctx, "side", MergeMode::Default, None).unwrap_err();
    assert!(error.message.contains("waiting"), "{}", error.message);
}

#[test]
fn gives_the_three_versions_of_a_conflicted_file() {
    let tmp = conflicting();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    merge(&ctx, "side", MergeMode::Default, None).unwrap();

    let file = conflict_file(&ctx, "a.txt").unwrap();
    assert_eq!(file.ours.as_deref(), Some("main top\nmiddle\nbottom\n"));
    assert_eq!(file.theirs.as_deref(), Some("side top\nmiddle\nbottom\n"));
    assert_eq!(file.base.as_deref(), Some("top\nmiddle\nbottom\n"));
    let worktree = file.worktree.unwrap();
    assert!(
        worktree.contains("<<<<<<<") && worktree.contains(">>>>>>>"),
        "{worktree}"
    );
    assert!(!file.choose_side_only);
}

#[test]
fn resolving_every_file_then_continuing_makes_the_merge_commit() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    merge(&ctx, "side", MergeMode::Default, None).unwrap();

    let early = continue_operation(&ctx).unwrap_err();
    assert_eq!(early.kind, ErrorKind::Conflict);
    assert_eq!(early.files, vec!["a.txt".to_string()]);

    let status = resolve_file(&ctx, "a.txt", "both tops\nmiddle\nbottom\n").unwrap();
    assert!(status.conflicted.is_empty());
    assert!(
        status.operation.is_some(),
        "still a merge until it is committed"
    );

    let status = continue_operation(&ctx).unwrap();
    assert!(status.operation.is_none());
    assert_eq!(parents(dir, "HEAD"), 2);
    assert_eq!(read(dir, "a.txt"), "both tops\nmiddle\nbottom\n");
    assert_eq!(read(dir, "side.txt"), "side\n");
    assert!(text(dir, &["log", "-1", "--format=%s"]).starts_with("Merge branch 'side'"));
}

#[test]
fn a_side_can_be_chosen_as_it_is() {
    for (side, expected) in [(Side::Ours, "main top\n"), (Side::Theirs, "side top\n")] {
        let tmp = repo_with(&[("a.txt", "top\n")]);
        let dir = tmp.path();
        git(dir, &["switch", "-q", "-c", "side"]);
        commit_file(dir, "a.txt", "side top\n", "side");
        git(dir, &["switch", "-q", "main"]);
        commit_file(dir, "a.txt", "main top\n", "main");
        let stores = RepoStores::default();
        let ctx = ctx(&tmp, &stores);
        merge(&ctx, "side", MergeMode::Default, None).unwrap();

        let status = resolve_side(&ctx, "a.txt", side).unwrap();
        assert!(status.conflicted.is_empty());
        assert_eq!(read(dir, "a.txt"), expected);
    }
}

#[test]
fn a_file_deleted_on_one_side_is_kept_or_removed_by_choosing() {
    for (side, exists) in [(Side::Ours, true), (Side::Theirs, false)] {
        let tmp = repo_with(&[("a.txt", "one\n"), ("keep.txt", "k\n")]);
        let dir = tmp.path();
        git(dir, &["switch", "-q", "-c", "side"]);
        git(dir, &["rm", "-q", "a.txt"]);
        git(dir, &["commit", "-q", "-m", "side deletes a"]);
        git(dir, &["switch", "-q", "main"]);
        commit_file(dir, "a.txt", "one edited\n", "main edits a");
        let stores = RepoStores::default();
        let ctx = ctx(&tmp, &stores);

        let result = merge(&ctx, "side", MergeMode::Default, None).unwrap();
        assert_eq!(result.outcome, MergeOutcome::Conflicts);
        assert_eq!(result.status.conflicted[0].code, "UD");
        let file = conflict_file(&ctx, "a.txt").unwrap();
        assert!(file.ours.is_some() && file.theirs.is_none());

        // "Theirs" deleted the file: choosing theirs deletes it, choosing ours keeps it.
        resolve_side(&ctx, "a.txt", side).unwrap();
        assert_eq!(dir.join("a.txt").exists(), exists);
        continue_operation(&ctx).unwrap();
    }
}

#[test]
fn binary_files_can_only_be_resolved_by_choosing_a_side() {
    let tmp = repo_with(&[("img.bin", "base")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    fs::write(dir.join("img.bin"), [0u8, 1, 2, 3]).unwrap();
    git(dir, &["commit", "-q", "-am", "side binary"]);
    git(dir, &["switch", "-q", "main"]);
    fs::write(dir.join("img.bin"), [0u8, 9, 9, 9]).unwrap();
    git(dir, &["commit", "-q", "-am", "main binary"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    merge(&ctx, "side", MergeMode::Default, None).unwrap();

    let file = conflict_file(&ctx, "img.bin").unwrap();
    assert!(file.choose_side_only);
    assert!(file.ours.is_none() && file.theirs.is_none() && file.worktree.is_none());
    resolve_side(&ctx, "img.bin", Side::Theirs).unwrap();
    assert_eq!(fs::read(dir.join("img.bin")).unwrap(), vec![0u8, 1, 2, 3]);
}

#[test]
fn aborting_puts_everything_back() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let head = text(dir, &["rev-parse", "HEAD"]);
    merge(&ctx, "side", MergeMode::Default, None).unwrap();
    resolve_file(&ctx, "a.txt", "half way\n").unwrap();

    let status = abort_operation(&ctx).unwrap();
    assert!(status.operation.is_none() && status.conflicted.is_empty());
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(read(dir, "a.txt"), "main top\nmiddle\nbottom\n");
    assert!(!dir.join("side.txt").exists());
}

#[test]
fn a_squash_that_conflicts_can_be_cancelled() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = merge(&ctx, "side", MergeMode::Squash, None).unwrap();
    assert_eq!(result.outcome, MergeOutcome::Conflicts);
    assert!(
        result.status.operation.is_none(),
        "a squash records no merge"
    );

    let status = abort_operation(&ctx).unwrap();
    assert!(status.conflicted.is_empty());
    assert_eq!(read(dir, "a.txt"), "main top\nmiddle\nbottom\n");
}

#[test]
fn nothing_to_cancel_is_an_error() {
    let tmp = ahead();
    let stores = RepoStores::default();
    assert!(abort_operation(&ctx(&tmp, &stores)).is_err());
}

#[test]
fn only_conflicted_files_inside_the_repository_can_be_resolved() {
    let tmp = conflicting();
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    merge(&ctx, "side", MergeMode::Default, None).unwrap();

    for path in ["../outside.txt", "/etc/hosts", "side.txt", "", "a/../a.txt"] {
        assert!(resolve_file(&ctx, path, "x").is_err(), "{path}");
        assert!(conflict_file(&ctx, path).is_err(), "{path}");
        assert!(resolve_side(&ctx, path, Side::Ours).is_err(), "{path}");
    }
    assert!(!dir.parent().unwrap().join("outside.txt").exists());
}

// ---------- operations started elsewhere ----------

#[test]
fn an_operation_started_in_a_terminal_is_found_from_the_git_directory() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "side"]);
    let edit = commit_file(dir, "a.txt", "side\n", "side edits a");
    git(dir, &["switch", "-q", "main"]);
    commit_file(dir, "a.txt", "main\n", "main edits a");
    // The terminal's own cherry-pick, which stops on the conflict.
    let _ = std::process::Command::new("git")
        .current_dir(dir)
        .args(["cherry-pick", &edit])
        .output();

    // A fresh application state (as after a restart) sees it.
    let stores = RepoStores::default();
    let status = ctx(&tmp, &stores).status().unwrap();
    assert_eq!(status.operation.unwrap().kind, OperationKind::CherryPick);
    assert_eq!(status.conflicted.len(), 1);

    resolve_file(&ctx(&tmp, &stores), "a.txt", "resolved\n").unwrap();
    let status = continue_operation(&ctx(&tmp, &stores)).unwrap();
    assert!(status.operation.is_none());
    assert_eq!(text(dir, &["log", "-1", "--format=%s"]), "side edits a");
}

#[test]
fn no_operation_is_reported_on_a_quiet_repository() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    assert!(ctx(&tmp, &stores).status().unwrap().operation.is_none());
}
