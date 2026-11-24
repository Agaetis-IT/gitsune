//! Linked working trees, on throwaway repositories.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
use super::repo::open_repository;
use super::status_tests::write;
use super::store::RepoStores;
use super::tests::git;
use super::worktrees::*;

fn ctx<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    OpCtx {
        workdir: tmp.path(),
        stores,
    }
}

fn branch_of(dir: &Path) -> String {
    String::from_utf8_lossy(&git_out(dir, &["rev-parse", "--abbrev-ref", "HEAD"]))
        .trim()
        .to_string()
}

/// A folder next to the repository, removed with the test.
fn sibling(holder: &TempDir, name: &str) -> PathBuf {
    holder.path().join(name)
}

#[test]
fn creates_lists_and_removes_a_worktree() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let holder = TempDir::new().unwrap();
    let stores = RepoStores::default();
    let target = sibling(&holder, "feature");

    add_worktree(
        &ctx(&tmp, &stores),
        target.to_str().unwrap(),
        "feat/x",
        true,
        None,
    )
    .unwrap();
    assert_eq!(branch_of(&target), "feat/x");

    let list = list_worktrees(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].is_main && list[0].branch.as_deref() == Some("main"));
    let linked = &list[1];
    assert_eq!(linked.branch.as_deref(), Some("feat/x"));
    assert_eq!(linked.dirty, Some(false));
    assert!(!linked.is_main);

    remove_worktree(&ctx(&tmp, &stores), target.to_str().unwrap(), false).unwrap();
    assert!(!target.exists());
    assert_eq!(list_worktrees(&ctx(&tmp, &stores)).unwrap().len(), 1);
}

#[test]
fn checks_an_existing_branch_out_and_refuses_one_used_elsewhere() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let holder = TempDir::new().unwrap();
    let stores = RepoStores::default();
    git(tmp.path(), &["branch", "topic"]);
    let first = sibling(&holder, "one");
    add_worktree(
        &ctx(&tmp, &stores),
        first.to_str().unwrap(),
        "topic",
        false,
        None,
    )
    .unwrap();

    let second = sibling(&holder, "two");
    let refused = add_worktree(
        &ctx(&tmp, &stores),
        second.to_str().unwrap(),
        "topic",
        false,
        None,
    );
    assert!(refused.is_err(), "a branch can be checked out once");
    assert!(!second.exists());
}

#[test]
fn a_worktree_with_changes_is_not_removed_without_force() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let holder = TempDir::new().unwrap();
    let stores = RepoStores::default();
    let target = sibling(&holder, "dirty");
    add_worktree(
        &ctx(&tmp, &stores),
        target.to_str().unwrap(),
        "wip",
        true,
        None,
    )
    .unwrap();
    write(&target, "new.txt", "untracked\n");

    let list = list_worktrees(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(list[1].dirty, Some(true));
    assert!(remove_worktree(&ctx(&tmp, &stores), target.to_str().unwrap(), false).is_err());
    assert!(target.exists());
    remove_worktree(&ctx(&tmp, &stores), target.to_str().unwrap(), true).unwrap();
    assert!(!target.exists());
}

#[test]
fn locks_unlocks_and_prunes_a_vanished_worktree() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let holder = TempDir::new().unwrap();
    let stores = RepoStores::default();
    let target = sibling(&holder, "gone");
    add_worktree(
        &ctx(&tmp, &stores),
        target.to_str().unwrap(),
        "gone",
        true,
        None,
    )
    .unwrap();

    lock_worktree(&ctx(&tmp, &stores), target.to_str().unwrap(), true).unwrap();
    assert!(
        list_worktrees(&ctx(&tmp, &stores)).unwrap()[1]
            .locked
            .is_some()
    );
    lock_worktree(&ctx(&tmp, &stores), target.to_str().unwrap(), false).unwrap();
    assert!(
        list_worktrees(&ctx(&tmp, &stores)).unwrap()[1]
            .locked
            .is_none()
    );

    std::fs::remove_dir_all(&target).unwrap();
    let list = list_worktrees(&ctx(&tmp, &stores)).unwrap();
    assert!(list[1].prunable.is_some());
    assert_eq!(list[1].dirty, None);
    prune_worktrees(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(list_worktrees(&ctx(&tmp, &stores)).unwrap().len(), 1);
}

#[test]
fn a_linked_worktree_opens_as_a_repository_and_says_so() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let holder = TempDir::new().unwrap();
    let stores = RepoStores::default();
    let target = sibling(&holder, "linked");
    add_worktree(
        &ctx(&tmp, &stores),
        target.to_str().unwrap(),
        "side",
        true,
        None,
    )
    .unwrap();

    let (_, main) = open_repository(tmp.path()).unwrap();
    let (_, linked) = open_repository(&target).unwrap();
    assert!(!main.is_linked);
    assert!(linked.is_linked);
    assert_eq!(linked.head_branch.as_deref(), Some("side"));
}

#[test]
fn names_that_look_like_options_are_refused() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    assert!(add_worktree(&ctx(&tmp, &stores), "-x", "b", true, None).is_err());
    assert!(
        add_worktree(
            &ctx(&tmp, &stores),
            "/tmp/ok-folder-for-test",
            "--b",
            true,
            None
        )
        .is_err()
    );
    assert!(remove_worktree(&ctx(&tmp, &stores), "--force", false).is_err());
}
