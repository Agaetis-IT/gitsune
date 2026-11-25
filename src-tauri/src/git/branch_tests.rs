//! Branches, checkout and tags, on throwaway repositories (remotes are local bare repositories).

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::branches::*;
#[cfg(unix)]
use super::cli::with_common_tool_dirs;
use super::cli::{ErrorKind, classify, indented_lines};
use super::ops::{OpCtx, restore_snapshot};
use super::ops_tests::{git_out, numbered, replace_line, repo_with};
use super::status::FileState;
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

fn branch(name: &str) -> CheckoutTarget {
    CheckoutTarget::Branch {
        name: name.to_string(),
    }
}

fn current(dir: &Path) -> String {
    text(dir, &["branch", "--show-current"])
}

/// main (a.txt, 12 lines) plus `other`, which changes line 1 of a.txt; HEAD is on main.
fn with_other_branch() -> TempDir {
    let tmp = repo_with(&[("a.txt", &numbered(12)), ("keep.txt", "keep\n")]);
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "other"]);
    write(
        dir,
        "a.txt",
        replace_line(&numbered(12), 1, "line 1 on other"),
    );
    git(dir, &["commit", "-q", "-am", "change on other"]);
    git(dir, &["switch", "-q", "main"]);
    tmp
}

/// A repository whose `origin` is a local bare repository with `main` pushed.
fn with_remote() -> (TempDir, TempDir) {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let url = remote.path().to_string_lossy().into_owned();
    git(tmp.path(), &["remote", "add", "origin", &url]);
    git(tmp.path(), &["push", "-q", "origin", "main"]);
    (tmp, remote)
}

// ---------- classification of git's messages ----------

#[test]
fn recognises_the_messages_git_prints() {
    let overwrite = "error: Your local changes to the following files would be overwritten by checkout:\n\ta.txt\n\tdir/b.txt\nPlease commit your changes or stash them before you switch branches.\nAborting\n";
    assert_eq!(
        classify("", overwrite),
        Some(ErrorKind::WouldOverwriteChanges)
    );
    assert_eq!(
        indented_lines(overwrite),
        vec!["a.txt".to_string(), "dir/b.txt".to_string()]
    );
    let untracked = "error: The following untracked working tree files would be overwritten by checkout:\n\tnew.txt\nPlease move or remove them before you switch branches.\n";
    assert_eq!(
        classify("", untracked),
        Some(ErrorKind::WouldOverwriteChanges)
    );
    assert_eq!(
        classify(
            "",
            "error: the branch 'x' is not fully merged.\nhint: If you are sure you want to delete it, run 'git branch -D x'"
        ),
        Some(ErrorKind::NotFullyMerged)
    );
    assert_eq!(
        classify(
            "",
            "error: Cannot delete branch 'main' checked out at '/tmp/repo'"
        ),
        Some(ErrorKind::BranchCheckedOut)
    );
    assert_eq!(
        classify("", "fatal: a branch named 'x' already exists"),
        Some(ErrorKind::AlreadyExists)
    );
    assert_eq!(
        classify("", "fatal: tag 'v1' already exists"),
        Some(ErrorKind::AlreadyExists)
    );
    assert_eq!(
        classify("", "fatal: 'a b' is not a valid branch name"),
        Some(ErrorKind::InvalidName)
    );
    assert_eq!(classify("", "something else"), None);
}

// ---------- creating and switching ----------

#[test]
fn creates_a_branch_at_head_at_a_commit_and_with_checkout() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let first = text(dir, &["rev-parse", "HEAD"]);
    write(dir, "a.txt", "two\n");
    git(dir, &["commit", "-q", "-am", "second"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    create_branch(&ctx, "here", None, false).unwrap();
    assert_eq!(
        text(dir, &["rev-parse", "here"]),
        text(dir, &["rev-parse", "HEAD"])
    );
    assert_eq!(current(dir), "main", "no checkout asked");

    create_branch(&ctx, "old", Some(&first), false).unwrap();
    assert_eq!(text(dir, &["rev-parse", "old"]), first);

    let status = create_branch(&ctx, "feature/new", Some("old"), true).unwrap();
    assert_eq!(status.branch.as_deref(), Some("feature/new"));
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), first);
    assert_eq!(
        stores.activity.list().last().unwrap().command,
        format!("git switch -c feature/new {first}")
    );
}

#[test]
fn refuses_bad_names_existing_branches_and_unknown_start_points() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    for bad in [
        "", " ", "a b", "-x", "a..b", "x.lock", "a~b", "bad:name", " pad",
    ] {
        let error = create_branch(&ctx, bad, None, false).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidName, "{bad:?}: {error}");
    }
    assert_eq!(
        create_branch(&ctx, "main", None, false).unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert!(create_branch(&ctx, "x", Some("no-such-commit"), false).is_err());
    assert!(stores.activity.list().is_empty(), "nothing was run");
}

#[test]
fn switches_to_a_branch_a_remote_branch_a_commit_and_a_tag() {
    let (tmp, _remote) = with_remote();
    let dir = tmp.path();
    let first = text(dir, &["rev-parse", "HEAD"]);
    git(dir, &["branch", "-q", "feature/x"]);
    git(dir, &["push", "-q", "origin", "feature/x"]);
    git(dir, &["branch", "-q", "-D", "feature/x"]); // only origin/feature/x is left
    git(dir, &["tag", "v1"]);
    write(dir, "a.txt", "two\n");
    git(dir, &["commit", "-q", "-am", "second"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    // A remote branch creates the local one and tracks it.
    let result = checkout(
        &ctx,
        &CheckoutTarget::Remote {
            name: "origin/feature/x".into(),
        },
        None,
    )
    .unwrap();
    assert_eq!(result.status.branch.as_deref(), Some("feature/x"));
    assert_eq!(
        text(dir, &["rev-parse", "--abbrev-ref", "feature/x@{upstream}"]),
        "origin/feature/x"
    );

    // With the local branch there already, it is simply switched to.
    checkout(&ctx, &branch("main"), None).unwrap();
    checkout(
        &ctx,
        &CheckoutTarget::Remote {
            name: "origin/feature/x".into(),
        },
        None,
    )
    .unwrap();
    assert_eq!(current(dir), "feature/x");

    // A commit and a tag give a detached HEAD.
    let detached = checkout(&ctx, &CheckoutTarget::Commit { id: first.clone() }, None).unwrap();
    assert!(detached.status.detached);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), first);
    checkout(&ctx, &branch("main"), None).unwrap();
    let tagged = checkout(&ctx, &CheckoutTarget::Tag { name: "v1".into() }, None).unwrap();
    assert!(tagged.status.detached);
    assert_eq!(text(dir, &["rev-parse", "HEAD"]), first);

    assert!(checkout(&ctx, &branch("nope"), None).is_err());
    assert!(
        checkout(
            &ctx,
            &CheckoutTarget::Tag {
                name: "nope".into()
            },
            None
        )
        .is_err()
    );
}

// ---------- checkout with local changes ----------

#[test]
fn changes_that_do_not_touch_the_target_come_along() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    write(dir, "keep.txt", "my edit\n"); // `other` does not change keep.txt
    write(dir, "untracked.txt", "mine\n");
    let stores = RepoStores::default();
    let result = checkout(&ctx(&tmp, &stores), &branch("other"), None).unwrap();
    assert_eq!(result.status.branch.as_deref(), Some("other"));
    assert_eq!(
        fs::read_to_string(dir.join("keep.txt")).unwrap(),
        "my edit\n"
    );
    assert!(dir.join("untracked.txt").exists());
}

#[test]
fn changes_in_the_way_are_reported_with_their_files_and_nothing_moves() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    write(dir, "a.txt", replace_line(&numbered(12), 12, "local edit"));
    let stores = RepoStores::default();

    let error = checkout(&ctx(&tmp, &stores), &branch("other"), None).unwrap_err();
    assert_eq!(error.kind, ErrorKind::WouldOverwriteChanges, "{error}");
    assert_eq!(error.files, vec!["a.txt".to_string()]);
    assert_eq!(current(dir), "main");
    assert!(
        fs::read_to_string(dir.join("a.txt"))
            .unwrap()
            .contains("local edit")
    );
}

#[test]
fn an_untracked_file_in_the_way_is_reported_too() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    git(dir, &["switch", "-q", "other"]);
    write(dir, "extra.txt", "tracked on other\n");
    git(dir, &["add", "extra.txt"]);
    git(dir, &["commit", "-q", "-m", "add extra"]);
    git(dir, &["switch", "-q", "main"]);
    write(dir, "extra.txt", "untracked on main\n");
    let stores = RepoStores::default();
    let error = checkout(&ctx(&tmp, &stores), &branch("other"), None).unwrap_err();
    assert_eq!(error.kind, ErrorKind::WouldOverwriteChanges);
    assert_eq!(error.files, vec!["extra.txt".to_string()]);
}

#[test]
fn stash_and_switch_brings_the_changes_back_staging_included() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    // Overlaps with `other` on the file, but on another line: the stash applies cleanly.
    write(dir, "a.txt", replace_line(&numbered(12), 12, "local edit"));
    git(dir, &["add", "a.txt"]);
    write(dir, "keep.txt", "unstaged edit\n");
    write(dir, "new.txt", "untracked\n");
    let stores = RepoStores::default();

    let result = checkout(
        &ctx(&tmp, &stores),
        &branch("other"),
        Some(DirtyStrategy::Stash),
    )
    .unwrap();
    assert!(result.stashed && !result.stash_conflict);
    assert_eq!(result.status.branch.as_deref(), Some("other"));
    assert_eq!(text(dir, &["stash", "list"]), "", "the stash was used up");
    let a = fs::read_to_string(dir.join("a.txt")).unwrap();
    assert!(
        a.contains("line 1 on other") && a.contains("local edit"),
        "both changes: {a}"
    );
    let status = result.status;
    assert_eq!(status.staged.len(), 1, "a.txt is still staged");
    assert!(status.unstaged.iter().any(|e| e.path == "keep.txt"));
    assert!(
        status
            .unstaged
            .iter()
            .any(|e| e.path == "new.txt" && e.state == FileState::Untracked)
    );
}

#[test]
fn a_stash_that_conflicts_stays_in_the_stash() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    write(
        dir,
        "a.txt",
        replace_line(&numbered(12), 1, "line 1 on main, edited locally"),
    );
    let stores = RepoStores::default();

    let result = checkout(
        &ctx(&tmp, &stores),
        &branch("other"),
        Some(DirtyStrategy::Stash),
    )
    .unwrap();
    assert!(result.stash_conflict && !result.stashed);
    assert_eq!(result.status.branch.as_deref(), Some("other"));
    assert_eq!(result.status.conflicted.len(), 1);
    assert_eq!(
        text(dir, &["stash", "list"]).lines().count(),
        1,
        "the changes are safe in the stash"
    );
}

#[test]
fn stash_and_switch_to_a_missing_branch_changes_nothing() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    write(dir, "keep.txt", "edit\n");
    let stores = RepoStores::default();
    assert!(
        checkout(
            &ctx(&tmp, &stores),
            &branch("nope"),
            Some(DirtyStrategy::Stash)
        )
        .is_err()
    );
    assert_eq!(text(dir, &["stash", "list"]), "");
    assert_eq!(fs::read_to_string(dir.join("keep.txt")).unwrap(), "edit\n");
}

#[test]
fn discard_and_switch_keeps_a_snapshot_to_get_the_changes_back() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    let edited = replace_line(&numbered(12), 12, "local edit");
    write(dir, "a.txt", &edited);
    git(dir, &["add", "a.txt"]);
    write(dir, "a.txt", replace_line(&edited, 6, "unstaged too"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let result = checkout(&ctx, &branch("other"), Some(DirtyStrategy::Discard)).unwrap();
    assert_eq!(result.status.branch.as_deref(), Some("other"));
    assert!(result.status.staged.is_empty() && result.status.unstaged.is_empty());
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        replace_line(&numbered(12), 1, "line 1 on other")
    );

    let snapshot = result.snapshot.expect("a snapshot is kept");
    assert_eq!(snapshot.paths, vec!["a.txt".to_string()]);
    restore_snapshot(&ctx, &snapshot.reference).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        replace_line(&edited, 6, "unstaged too")
    );
}

#[test]
fn a_clean_tree_needs_no_strategy() {
    let tmp = with_other_branch();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let stashed = checkout(&ctx, &branch("other"), Some(DirtyStrategy::Stash)).unwrap();
    assert!(!stashed.stashed && stashed.snapshot.is_none());
    let discarded = checkout(&ctx, &branch("main"), Some(DirtyStrategy::Discard)).unwrap();
    assert!(discarded.snapshot.is_none());
}

// ---------- rename, delete, upstream ----------

#[test]
fn renames_a_branch_including_the_current_one() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["branch", "old"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    rename_branch(&ctx, "old", "feature/renamed").unwrap();
    assert!(text(dir, &["branch", "--list", "feature/renamed"]).contains("feature/renamed"));
    assert_eq!(text(dir, &["branch", "--list", "old"]), "");

    let status = rename_branch(&ctx, "main", "trunk").unwrap();
    assert_eq!(status.branch.as_deref(), Some("trunk"));

    assert_eq!(
        rename_branch(&ctx, "feature/renamed", "trunk")
            .unwrap_err()
            .kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        rename_branch(&ctx, "feature/renamed", "bad name")
            .unwrap_err()
            .kind,
        ErrorKind::InvalidName
    );
    assert!(rename_branch(&ctx, "ghost", "x").is_err());
}

#[test]
fn deletes_merged_branches_and_warns_about_unmerged_ones() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["branch", "merged"]);
    git(dir, &["switch", "-q", "-c", "unmerged"]);
    write(dir, "b.txt", "only here\n");
    git(dir, &["add", "b.txt"]);
    git(dir, &["commit", "-q", "-m", "only on unmerged"]);
    let tip = text(dir, &["rev-parse", "HEAD"]);
    git(dir, &["switch", "-q", "main"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let deleted = delete_branch(&ctx, "merged", false, false).unwrap();
    assert_eq!(deleted.name, "merged");
    assert_eq!(text(dir, &["branch", "--list", "merged"]), "");

    let refused = delete_branch(&ctx, "unmerged", false, false).unwrap_err();
    assert_eq!(refused.kind, ErrorKind::NotFullyMerged, "{refused}");
    assert!(
        !text(dir, &["branch", "--list", "unmerged"]).is_empty(),
        "still there"
    );

    let forced = delete_branch(&ctx, "unmerged", true, false).unwrap();
    assert_eq!(forced.tip, tip);
    assert_eq!(text(dir, &["branch", "--list", "unmerged"]), "");

    assert_eq!(
        delete_branch(&ctx, "main", true, false).unwrap_err().kind,
        ErrorKind::BranchCheckedOut
    );
    assert!(delete_branch(&ctx, "ghost", false, false).is_err());
}

#[test]
fn a_deleted_branch_comes_back_with_its_tip_and_upstream() {
    let (tmp, _remote) = with_remote();
    let dir = tmp.path();
    git(dir, &["switch", "-q", "-c", "topic"]);
    write(dir, "t.txt", "topic\n");
    git(dir, &["add", "t.txt"]);
    git(dir, &["commit", "-q", "-m", "topic work"]);
    git(dir, &["push", "-q", "-u", "origin", "topic"]);
    git(dir, &["switch", "-q", "main"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let deleted = delete_branch(&ctx, "topic", true, false).unwrap();
    assert_eq!(deleted.upstream.as_deref(), Some("origin/topic"));
    restore_branch(&ctx, "topic", &deleted.tip, deleted.upstream.as_deref()).unwrap();
    assert_eq!(text(dir, &["rev-parse", "topic"]), deleted.tip);
    assert_eq!(
        text(dir, &["rev-parse", "--abbrev-ref", "topic@{upstream}"]),
        "origin/topic"
    );

    assert_eq!(
        restore_branch(&ctx, "topic", &deleted.tip, None)
            .unwrap_err()
            .kind,
        ErrorKind::AlreadyExists
    );
    assert!(
        restore_branch(
            &ctx,
            "other",
            "0123456789abcdef0123456789abcdef01234567",
            None
        )
        .is_err()
    );
}

#[test]
fn sets_and_unsets_the_upstream() {
    let (tmp, _remote) = with_remote();
    let dir = tmp.path();
    git(dir, &["branch", "topic"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    set_upstream(&ctx, "topic", Some("origin/main")).unwrap();
    assert_eq!(
        text(dir, &["rev-parse", "--abbrev-ref", "topic@{upstream}"]),
        "origin/main"
    );
    set_upstream(&ctx, "topic", None).unwrap();
    assert_eq!(text(dir, &["config", "--get", "branch.topic.remote"]), "");

    assert!(set_upstream(&ctx, "topic", Some("origin/nope")).is_err());
    assert!(set_upstream(&ctx, "ghost", Some("origin/main")).is_err());
}

// ---------- tags ----------

#[test]
fn creates_lightweight_and_annotated_tags() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let first = text(dir, &["rev-parse", "HEAD"]);
    write(dir, "a.txt", "two\n");
    git(dir, &["commit", "-q", "-am", "second"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    create_tag(&ctx, "light", Some(&first), None).unwrap();
    assert_eq!(text(dir, &["cat-file", "-t", "light"]), "commit");
    assert_eq!(text(dir, &["rev-parse", "light"]), first);

    create_tag(&ctx, "v1.0.0", None, Some("Release one\n\nWith notes")).unwrap();
    assert_eq!(text(dir, &["cat-file", "-t", "v1.0.0"]), "tag");
    assert_eq!(
        text(dir, &["tag", "-n9", "-l", "v1.0.0"])
            .split_whitespace()
            .next(),
        Some("v1.0.0")
    );
    assert!(text(dir, &["cat-file", "-p", "v1.0.0"]).contains("With notes"));
    assert_eq!(
        text(dir, &["rev-parse", "v1.0.0^{commit}"]),
        text(dir, &["rev-parse", "HEAD"])
    );

    // A blank message is a lightweight tag.
    create_tag(&ctx, "blank", None, Some("   ")).unwrap();
    assert_eq!(text(dir, &["cat-file", "-t", "blank"]), "commit");

    assert_eq!(
        create_tag(&ctx, "light", None, None).unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        create_tag(&ctx, "bad name", None, None).unwrap_err().kind,
        ErrorKind::InvalidName
    );
    assert!(create_tag(&ctx, "x", Some("no-such-commit"), None).is_err());
}

#[test]
fn an_annotated_tag_works_without_a_configured_identity() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["config", "--unset", "user.name"]);
    git(dir, &["config", "--unset", "user.email"]);
    git(dir, &["config", "user.useConfigOnly", "true"]);
    let stores = RepoStores::with_env(vec![
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ]);
    create_tag(&ctx(&tmp, &stores), "v1", None, Some("A release")).unwrap();
    assert_eq!(text(dir, &["cat-file", "-t", "v1"]), "tag");
    assert!(text(dir, &["cat-file", "-p", "v1"]).contains("tagger gitsune"));
}

#[test]
fn a_deleted_tag_comes_back_exactly_as_it_was() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    git(dir, &["tag", "-a", "v1", "-m", "annotated"]);
    git(dir, &["tag", "light"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    for name in ["v1", "light"] {
        let before_type = text(dir, &["cat-file", "-t", name]);
        let before_id = text(dir, &["rev-parse", name]);
        let deleted = delete_tag(&ctx, name).unwrap();
        assert_eq!(text(dir, &["tag", "--list", name]), "");
        assert_eq!(deleted.target, before_id);
        restore_tag(&ctx, name, &deleted.target).unwrap();
        assert_eq!(text(dir, &["rev-parse", name]), before_id);
        assert_eq!(text(dir, &["cat-file", "-t", name]), before_type);
    }
    assert!(delete_tag(&ctx, "ghost").is_err());
    assert_eq!(
        restore_tag(&ctx, "v1", "x").unwrap_err().kind,
        ErrorKind::AlreadyExists
    );
}

// ---------- safety ----------

#[test]
fn every_operation_is_refused_in_read_only_mode_without_running_git() {
    let tmp = with_other_branch();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let ctx = ctx(&tmp, &stores);
    let kind = |r: Result<(), super::cli::GitError>| r.unwrap_err().kind;

    assert_eq!(
        kind(create_branch(&ctx, "x", None, false).map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(
        kind(checkout(&ctx, &branch("other"), None).map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(
        kind(rename_branch(&ctx, "other", "y").map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(
        kind(delete_branch(&ctx, "other", true, false).map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(
        kind(set_upstream(&ctx, "other", None).map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(
        kind(create_tag(&ctx, "t", None, None).map(drop)),
        ErrorKind::ReadOnly
    );
    assert_eq!(kind(delete_tag(&ctx, "t").map(drop)), ErrorKind::ReadOnly);
    assert_eq!(
        kind(restore_branch(&ctx, "z", "HEAD", None).map(drop)),
        ErrorKind::ReadOnly
    );
    assert!(stores.activity.list().is_empty());
}

#[test]
fn a_lock_held_by_another_process_stops_a_checkout_without_being_removed() {
    let tmp = with_other_branch();
    let dir = tmp.path();
    let lock = dir.join(".git/index.lock");
    fs::write(&lock, "").unwrap();
    let stores = RepoStores::default();
    let error = checkout(&ctx(&tmp, &stores), &branch("other"), None).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Locked, "{error}");
    assert!(lock.exists());
    assert_eq!(current(dir), "main");
}

#[cfg(target_os = "macos")]
#[test]
fn the_usual_tool_folders_are_added_to_a_short_path() {
    let short = "/usr/bin:/bin";
    let extended = with_common_tool_dirs(short);
    assert!(extended.starts_with("/usr/bin:/bin:"), "{extended}");
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        assert!(extended.split(':').any(|p| p == dir), "{dir}");
    }
    // Nothing is added twice, and the order of what was there is kept.
    assert_eq!(with_common_tool_dirs(&extended), extended);
    assert_eq!(
        with_common_tool_dirs("/opt/homebrew/bin:/usr/bin")
            .matches("/opt/homebrew/bin")
            .count(),
        1
    );
    assert!(with_common_tool_dirs("").starts_with("/opt/homebrew/bin"));
}

#[cfg(unix)]
#[test]
fn the_toolchain_folders_of_the_home_directory_are_added() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let extended = with_common_tool_dirs("/usr/bin");
    let bun = format!("{}/.bun/bin", home.trim_end_matches('/'));
    assert!(extended.split(':').any(|p| p == bun), "{extended}");
}

#[test]
fn the_identity_agent_of_an_ssh_config_is_found() {
    use super::cli::identity_agent;
    let config = "Host *\n\tIdentityAgent \"~/Library/Group Containers/x/agent.sock\"\n";
    assert_eq!(
        identity_agent(config, "/Users/me/").as_deref(),
        Some("/Users/me/Library/Group Containers/x/agent.sock")
    );
    assert_eq!(
        identity_agent("identityagent = /tmp/a.sock", "/h").as_deref(),
        Some("/tmp/a.sock")
    );
    assert_eq!(identity_agent("IdentityAgent none", "/h"), None);
    assert_eq!(identity_agent("Host a\n  User git", "/h"), None);
}

#[test]
fn deleting_a_branch_can_remove_the_remote_tracking_branch_it_followed() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let tip = text(dir, &["rev-parse", "HEAD"]);
    git(dir, &["branch", "topic"]);
    git(dir, &["remote", "add", "origin", "."]);
    git(dir, &["update-ref", "refs/remotes/origin/topic", &tip]);
    git(dir, &["config", "branch.topic.remote", "origin"]);
    git(dir, &["config", "branch.topic.merge", "refs/heads/topic"]);
    let stores = RepoStores::default();

    let deleted = delete_branch(&ctx(&tmp, &stores), "topic", false, true).unwrap();
    assert_eq!(deleted.upstream.as_deref(), Some("origin/topic"));
    assert!(!dir.join(".git/refs/remotes/origin/topic").exists());
    assert!(!text(dir, &["branch", "--list", "topic"]).contains("topic"));
}

#[test]
fn deleting_a_branch_keeps_its_remote_tracking_branch_by_default() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let dir = tmp.path();
    let tip = text(dir, &["rev-parse", "HEAD"]);
    git(dir, &["branch", "topic"]);
    git(dir, &["update-ref", "refs/remotes/origin/topic", &tip]);
    git(dir, &["config", "branch.topic.remote", "origin"]);
    git(dir, &["config", "branch.topic.merge", "refs/heads/topic"]);
    let stores = RepoStores::default();

    delete_branch(&ctx(&tmp, &stores), "topic", false, false).unwrap();
    assert!(dir.join(".git/refs/remotes/origin/topic").exists());
}
