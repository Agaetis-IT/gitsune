//! Commits, amends, hooks, identity, signing, discarding changes and recovering them.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::cli::ErrorKind;
use super::ops::{
    MAX_SNAPSHOTS, OpCtx, SNAPSHOT_PREFIX, commit, discard_paths, identity, list_snapshots,
    restore_snapshot, set_identity, stage_paths, undo_commit,
};
use super::ops_tests::{git_out, numbered, replace_line, repo_with};
use super::status::FileState;
use super::status_tests::{init_repo, write};
use super::store::RepoStores;
use super::tests::git;

fn paths(items: &[&str]) -> Vec<String> {
    items.iter().map(|p| p.to_string()).collect()
}

fn ctx<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    OpCtx {
        workdir: tmp.path(),
        stores,
    }
}

fn head(dir: &Path) -> String {
    String::from_utf8(git_out(dir, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_string()
}

fn message_of(dir: &Path, rev: &str) -> String {
    String::from_utf8(git_out(dir, &["log", "-1", "--format=%B", rev])).unwrap()
}

fn count(dir: &Path) -> usize {
    String::from_utf8(git_out(dir, &["rev-list", "--count", "HEAD"]))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn hook(dir: &Path, name: &str, script: &str) {
    let path = dir.join(".git/hooks").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    // On Windows git runs a hook through its own `sh` whatever the file mode.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

// ---------- committing ----------

#[test]
fn commits_the_index_with_a_summary_and_a_body() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let first = head(dir);
    write(dir, "a.txt", "two\n");
    write(dir, "unstaged.txt", "left alone\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();

    let result = commit(
        &ctx,
        "  Change a  ",
        "Explains why.\n\nTwo paragraphs.\n\n",
        false,
    )
    .unwrap();
    assert_eq!(result.id, head(dir));
    assert_eq!(result.before.as_deref(), Some(first.as_str()));
    assert_eq!(
        message_of(dir, "HEAD"),
        "Change a\n\nExplains why.\n\nTwo paragraphs.\n\n"
    );
    assert!(result.status.staged.is_empty());
    assert_eq!(
        result.status.unstaged.len(),
        1,
        "unstaged files stay in the working tree"
    );
    assert_eq!(
        stores.activity.list().last().unwrap().command,
        "git commit -F - < [41 bytes]"
    );
}

#[test]
fn a_summary_alone_gives_a_one_line_message() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    write(tmp.path(), "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    commit(&ctx, "Just a summary", "   \n", false).unwrap();
    assert_eq!(message_of(tmp.path(), "HEAD"), "Just a summary\n\n");
}

#[test]
fn the_first_commit_of_a_repository_works() {
    let tmp = init_repo();
    write(tmp.path(), "a.txt", "one\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let result = commit(&ctx, "Initial commit", "", false).unwrap();
    assert_eq!(result.before, None);
    assert_eq!(result.id, head(tmp.path()));
}

#[test]
fn amending_replaces_the_last_commit() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let made = commit(&ctx, "Second", "", false).unwrap();
    let parent = String::from_utf8(git_out(dir, &["rev-parse", "HEAD~1"])).unwrap();

    write(dir, "a.txt", "three\n");
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let amended = commit(&ctx, "Second, better", "now with more", true).unwrap();

    assert_eq!(count(dir), 2, "no extra commit");
    assert_ne!(amended.id, made.id);
    assert_eq!(amended.before.as_deref(), Some(made.id.as_str()));
    assert_eq!(
        String::from_utf8(git_out(dir, &["rev-parse", "HEAD~1"])).unwrap(),
        parent
    );
    assert_eq!(
        String::from_utf8(git_out(dir, &["show", "HEAD:a.txt"])).unwrap(),
        "three\n"
    );
    assert_eq!(
        message_of(dir, "HEAD"),
        "Second, better\n\nnow with more\n\n"
    );
}

#[test]
fn only_the_message_can_be_amended() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    commit(&ctx(&tmp, &stores), "A better message", "", true).unwrap();
    assert_eq!(message_of(tmp.path(), "HEAD"), "A better message\n\n");
    assert_eq!(count(tmp.path()), 1);
}

#[test]
fn nothing_to_commit_an_empty_summary_and_nothing_to_amend_are_refused() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    assert_eq!(
        commit(&ctx, "Nothing", "", false).unwrap_err().kind,
        ErrorKind::NothingToCommit
    );
    let empty = commit(&ctx, "   ", "body", false).unwrap_err();
    assert!(empty.message.contains("summary"));

    let fresh = init_repo();
    let fresh_stores = RepoStores::default();
    let error = commit(
        &OpCtx {
            workdir: fresh.path(),
            stores: &fresh_stores,
        },
        "x",
        "",
        true,
    )
    .unwrap_err();
    assert!(error.message.contains("no commit to amend"), "{error}");
}

#[test]
fn a_commit_can_be_undone_and_its_changes_come_back_staged() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let first = head(dir);
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let made = commit(&ctx, "Second", "", false).unwrap();

    let status = undo_commit(&ctx, made.before.as_deref(), &made.id).unwrap();
    assert_eq!(head(dir), first);
    assert_eq!(status.staged.len(), 1);
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "two\n");

    // HEAD moved on: refuse, nothing changes.
    commit(&ctx, "Again", "", false).unwrap();
    let error = undo_commit(&ctx, made.before.as_deref(), &made.id).unwrap_err();
    assert!(error.message.contains("HEAD has moved"), "{error}");
}

#[test]
fn undoing_the_first_commit_goes_back_to_no_commits() {
    let tmp = init_repo();
    write(tmp.path(), "a.txt", "one\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let made = commit(&ctx, "Initial commit", "", false).unwrap();
    let status = undo_commit(&ctx, made.before.as_deref(), &made.id).unwrap();
    assert_eq!(status.head, None);
    assert_eq!(status.staged.len(), 1);
}

// ---------- hooks, identity, signing ----------

#[test]
fn a_hook_that_rejects_the_commit_is_reported_with_its_output() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    hook(
        dir,
        "pre-commit",
        "echo 'lint failed: trailing whitespace' >&2\nexit 1",
    );
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let before = head(dir);

    let error = commit(&ctx, "Rejected", "", false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::HookFailed);
    assert!(
        error.message.contains("lint failed: trailing whitespace"),
        "{error}"
    );
    assert_eq!(head(dir), before, "no commit was made");
    assert!(!stores.activity.list().last().unwrap().ok);

    // The same hook letting the commit through.
    hook(dir, "pre-commit", "exit 0");
    assert!(commit(&ctx, "Accepted", "", false).is_ok());
}

#[test]
fn a_hook_cannot_wait_for_input() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    hook(
        dir,
        "pre-commit",
        "read answer\n[ -n \"$answer\" ] || exit 1",
    );
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    // Standard input is closed: the hook fails instead of hanging.
    assert_eq!(
        commit(&ctx, "Prompting", "", false).unwrap_err().kind,
        ErrorKind::HookFailed
    );
}

#[test]
fn a_missing_identity_is_detected_and_can_be_set() {
    let tmp = init_repo();
    let dir = tmp.path();
    git(dir, &["config", "--unset", "user.name"]);
    git(dir, &["config", "--unset", "user.email"]);
    git(dir, &["config", "user.useConfigOnly", "true"]);
    write(dir, "a.txt", "one\n");
    // No global or system configuration to fall back on.
    let isolated = RepoStores::with_env(vec![
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ]);
    let ctx = ctx(&tmp, &isolated);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();

    assert_eq!(identity(&ctx).unwrap().name, None);
    let error = commit(&ctx, "No author", "", false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsIdentity, "{error}");

    let set = set_identity(&ctx, "Alice Fakename", "alice@example.invalid", false).unwrap();
    assert_eq!(set.name.as_deref(), Some("Alice Fakename"));
    assert!(commit(&ctx, "With author", "", false).is_ok());
    let author = String::from_utf8(git_out(dir, &["log", "-1", "--format=%an <%ae>"])).unwrap();
    assert_eq!(author.trim(), "Alice Fakename <alice@example.invalid>");
}

#[test]
fn an_incomplete_identity_is_refused() {
    let tmp = init_repo();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    assert!(set_identity(&ctx, "", "a@example.invalid", false).is_err());
    assert!(set_identity(&ctx, "Alice", "  ", false).is_err());
    assert!(set_identity(&ctx, "Ali\nce", "a@example.invalid", false).is_err());
    assert!(stores.activity.list().is_empty());
}

#[test]
fn a_signing_failure_is_reported_as_such() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["config", "commit.gpgsign", "true"]);
    git(dir, &["config", "gpg.format", "openpgp"]); // not the ssh format a global config may set
    git(dir, &["config", "gpg.program", "/usr/bin/false"]);
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let before = head(dir);
    let error = commit(&ctx, "Unsigned", "", false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::SigningFailed, "{error}");
    assert_eq!(head(dir), before);
}

#[test]
fn commits_are_refused_in_read_only_mode_and_while_locked() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", "two\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();

    stores.set_read_only(true);
    assert_eq!(
        commit(&ctx, "No", "", false).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        undo_commit(&ctx, None, "x").unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        set_identity(&ctx, "A", "a@example.invalid", false)
            .unwrap_err()
            .kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        discard_paths(&ctx, &paths(&["a.txt"])).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    stores.set_read_only(false);

    fs::write(dir.join(".git/index.lock"), "").unwrap();
    assert_eq!(
        commit(&ctx, "Locked", "", false).unwrap_err().kind,
        ErrorKind::Locked
    );
    assert!(dir.join(".git/index.lock").exists());
}

// ---------- discarding and recovering ----------

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir.join(".git"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("gitsune-snapshot"))
        .collect()
}

#[test]
fn discarding_restores_the_staged_content_and_keeps_a_snapshot() {
    let original = numbered(10);
    let tmp = repo_with(&[("a.txt", &original), ("b.txt", "keep me\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", replace_line(&original, 2, "staged"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let staged_content = replace_line(&original, 2, "staged");
    write(
        dir,
        "a.txt",
        replace_line(&staged_content, 8, "unstaged edit"),
    );
    write(dir, "b.txt", "changed\n");
    let head_before = head(dir);
    let index_before = git_out(dir, &["ls-files", "-s"]);

    let result = discard_paths(&ctx, &paths(&["a.txt"])).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        staged_content
    );
    assert_eq!(
        fs::read_to_string(dir.join("b.txt")).unwrap(),
        "changed\n",
        "other files are untouched"
    );
    assert_eq!(
        git_out(dir, &["ls-files", "-s"]),
        index_before,
        "the index is untouched"
    );
    assert_eq!(head(dir), head_before);
    assert_eq!(result.status.staged.len(), 1, "the staged change stays");
    let snapshot = result.snapshot.unwrap();
    assert_eq!(snapshot.paths, vec!["a.txt".to_string()]);
    assert!(snapshot.reference.starts_with(SNAPSHOT_PREFIX));
    assert!(leftovers(dir).is_empty(), "the temporary index is removed");
    assert_eq!(list_snapshots(&ctx).unwrap().len(), 1);
}

#[test]
fn a_snapshot_kept_under_the_old_name_of_the_application_is_still_listed_and_restored() {
    use super::ops::LEGACY_SNAPSHOT_PREFIX;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", "edited\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let result = discard_paths(&ctx, &paths(&["a.txt"])).unwrap();
    let new_ref = result.snapshot.unwrap().reference;
    assert!(new_ref.starts_with(SNAPSHOT_PREFIX));

    // The same snapshot, as a version of the application before the rename would have left it.
    let legacy_ref = new_ref.replace(SNAPSHOT_PREFIX, LEGACY_SNAPSHOT_PREFIX);
    let id = String::from_utf8_lossy(&git_out(dir, &["rev-parse", &new_ref]))
        .trim()
        .to_string();
    git(dir, &["update-ref", &legacy_ref, &id]);
    git(dir, &["update-ref", "-d", &new_ref]);

    let listed = list_snapshots(&ctx).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].reference, legacy_ref);
    assert!(
        listed[0].description.starts_with("before"),
        "{}",
        listed[0].description
    );

    restore_snapshot(&ctx, &legacy_ref).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "edited\n");
}

#[test]
fn discarding_nothing_leaves_no_snapshot() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let result = discard_paths(&ctx(&tmp, &stores), &paths(&["a.txt", "ghost.txt"])).unwrap();
    assert!(result.snapshot.is_none());
    assert!(list_snapshots(&ctx(&tmp, &stores)).unwrap().is_empty());
}

#[test]
fn every_kind_of_discard_can_be_recovered() {
    let tmp = repo_with(&[("edit.txt", "original\n"), ("gone.txt", "will vanish\n")]);
    let dir = tmp.path();
    write(dir, "edit.txt", "my precious edit\n");
    fs::remove_file(dir.join("gone.txt")).unwrap();
    write(dir, "new.txt", "an untracked file\n");
    write(dir, "newdir/one.txt", "in a new folder\n");
    write(dir, "newdir/two.txt", "also\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let all = paths(&["edit.txt", "gone.txt", "new.txt", "newdir/"]);
    let result = discard_paths(&ctx, &all).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("edit.txt")).unwrap(),
        "original\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("gone.txt")).unwrap(),
        "will vanish\n"
    );
    assert!(!dir.join("new.txt").exists() && !dir.join("newdir").exists());
    assert!(
        ctx.status().unwrap().unstaged.is_empty(),
        "everything was discarded"
    );

    let snapshot = result.snapshot.unwrap();
    let restored = restore_snapshot(&ctx, &snapshot.reference).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("edit.txt")).unwrap(),
        "my precious edit\n"
    );
    assert!(!dir.join("gone.txt").exists(), "the deletion is back too");
    assert_eq!(
        fs::read_to_string(dir.join("new.txt")).unwrap(),
        "an untracked file\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("newdir/one.txt")).unwrap(),
        "in a new folder\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("newdir/two.txt")).unwrap(),
        "also\n"
    );

    // The consumed snapshot is gone; restoring saved what it overwrote, so it can be undone.
    let after = list_snapshots(&ctx).unwrap();
    assert!(after.iter().all(|s| s.reference != snapshot.reference));
    assert_eq!(after.len(), 1);
    assert_eq!(restored.snapshot.unwrap().reference, after[0].reference);
    let states: Vec<_> = restored.status.unstaged.iter().map(|e| e.state).collect();
    assert!(states.contains(&FileState::Untracked) && states.contains(&FileState::Deleted));
}

#[test]
fn restoring_never_loses_what_it_overwrites() {
    let tmp = repo_with(&[("a.txt", "original\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", "first edit\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let discarded = discard_paths(&ctx, &paths(&["a.txt"]))
        .unwrap()
        .snapshot
        .unwrap();

    write(dir, "a.txt", "an edit made after the discard\n");
    let restored = restore_snapshot(&ctx, &discarded.reference)
        .unwrap()
        .snapshot
        .unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "first edit\n"
    );

    // Undoing the restore brings the later edit back.
    restore_snapshot(&ctx, &restored.reference).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "an edit made after the discard\n"
    );
}

#[test]
fn only_snapshots_of_this_application_can_be_restored() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    assert!(restore_snapshot(&ctx, "refs/heads/main").is_err());
    assert!(restore_snapshot(&ctx, "refs/gitsune/undo/../../heads/main").is_err());
    assert!(restore_snapshot(&ctx, "refs/gitsune/undo/0000000000000-000000").is_err());
}

#[test]
fn discarding_works_before_the_first_commit() {
    let tmp = init_repo();
    let dir = tmp.path();
    write(dir, "a.txt", "one\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let snapshot = discard_paths(&ctx, &paths(&["a.txt"]))
        .unwrap()
        .snapshot
        .unwrap();
    assert!(!dir.join("a.txt").exists());
    restore_snapshot(&ctx, &snapshot.reference).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
}

#[test]
fn only_the_most_recent_snapshots_are_kept() {
    let tmp = repo_with(&[("a.txt", "base\n")]);
    let dir = tmp.path();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    for i in 0..(MAX_SNAPSHOTS + 3) {
        write(dir, "a.txt", format!("edit {i}\n"));
        discard_paths(&ctx, &paths(&["a.txt"])).unwrap();
    }
    let kept = list_snapshots(&ctx).unwrap();
    assert_eq!(kept.len(), MAX_SNAPSHOTS);
    // Newest first: the last discard is at the top.
    restore_snapshot(&ctx, &kept[0].reference).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        format!("edit {}\n", MAX_SNAPSHOTS + 2)
    );
}
