//! Linked working trees (`git worktree`): the same repository checked out in several folders.

use std::path::Path;

use serde::Serialize;

use super::cli::{ErrorKind, GitError, RunOptions};
use super::ops::OpCtx;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    /// The commit checked out there.
    pub head: String,
    /// Short branch name, `None` for a detached HEAD or a bare repository.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    /// The first one listed is the main working tree.
    pub is_main: bool,
    /// `Some(reason)` when locked (the reason may be empty).
    pub locked: Option<String>,
    /// `Some(reason)` when git would prune it: its folder is gone.
    pub prunable: Option<String>,
    /// Uncommitted changes there; `None` when it could not be told (folder gone, bare).
    pub dirty: Option<bool>,
}

/// Reads the output of `git worktree list --porcelain`.
pub fn parse_porcelain(text: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    for block in text.split("\n\n") {
        let mut tree = Worktree::default();
        let mut seen = false;
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => {
                    tree.path = value.to_string();
                    seen = true;
                }
                "HEAD" => tree.head = value.to_string(),
                "branch" => {
                    tree.branch = Some(
                        value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value)
                            .to_string(),
                    )
                }
                "detached" => tree.detached = true,
                "bare" => tree.bare = true,
                "locked" => tree.locked = Some(value.to_string()),
                "prunable" => tree.prunable = Some(value.to_string()),
                _ => {}
            }
        }
        if seen {
            tree.is_main = list.is_empty();
            list.push(tree);
        }
    }
    list
}

fn has_changes(ctx: &OpCtx, path: &str) -> Option<bool> {
    let output = ctx
        .runner()
        .run_raw(
            Path::new(path),
            &["status", "--porcelain", "--untracked-files=normal"],
            RunOptions::background(),
        )
        .ok()?;
    (output.code == 0).then(|| !output.stdout_text().trim().is_empty())
}

pub fn list_worktrees(ctx: &OpCtx) -> Result<Vec<Worktree>, GitError> {
    let output = ctx.runner().run(
        ctx.workdir,
        &["worktree", "list", "--porcelain"],
        RunOptions::background(),
    )?;
    let mut list = parse_porcelain(&output.stdout_text());
    for tree in &mut list {
        if !tree.bare && tree.prunable.is_none() {
            tree.dirty = has_changes(ctx, &tree.path);
        }
    }
    Ok(list)
}

fn check_arg(kind: &str, value: &str) -> Result<(), GitError> {
    if value.trim().is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        return Err(GitError::new(
            ErrorKind::InvalidName,
            format!("'{value}' is not a valid {kind}."),
        ));
    }
    Ok(())
}

/// Checks a branch out in a new folder. With `new_branch` the branch is created there, from
/// `start` (HEAD when empty); otherwise `branch` must exist and be free.
pub fn add_worktree(
    ctx: &OpCtx,
    path: &str,
    branch: &str,
    new_branch: bool,
    start: Option<&str>,
) -> Result<(), GitError> {
    check_arg("folder", path)?;
    check_arg("branch name", branch)?;
    let _write = ctx.stores.begin_write()?;
    let mut args = vec!["worktree", "add"];
    if new_branch {
        args.extend(["-b", branch, path]);
        if let Some(start) = start.filter(|s| !s.trim().is_empty()) {
            check_arg("starting point", start)?;
            args.push(start);
        }
    } else {
        args.extend([path, branch]);
    }
    ctx.runner()
        .run(ctx.workdir, &args, RunOptions::default())?;
    Ok(())
}

/// Removes a linked working tree. Without `force` git refuses when it has changes.
pub fn remove_worktree(ctx: &OpCtx, path: &str, force: bool) -> Result<(), GitError> {
    check_arg("folder", path)?;
    let _write = ctx.stores.begin_write()?;
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path);
    ctx.runner()
        .run(ctx.workdir, &args, RunOptions::default())?;
    Ok(())
}

pub fn lock_worktree(ctx: &OpCtx, path: &str, lock: bool) -> Result<(), GitError> {
    check_arg("folder", path)?;
    let _write = ctx.stores.begin_write()?;
    let verb = if lock { "lock" } else { "unlock" };
    ctx.runner().run(
        ctx.workdir,
        &["worktree", verb, path],
        RunOptions::default(),
    )?;
    Ok(())
}

/// Forgets the working trees whose folder is gone.
pub fn prune_worktrees(ctx: &OpCtx) -> Result<(), GitError> {
    let _write = ctx.stores.begin_write()?;
    ctx.runner()
        .run(ctx.workdir, &["worktree", "prune"], RunOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn the_porcelain_list_is_read() {
        let text = "worktree /work/app\nHEAD aaaa\nbranch refs/heads/main\n\n\
                    worktree /work/app-x\nHEAD bbbb\nbranch refs/heads/feat/x\nlocked in use\n\n\
                    worktree /work/old\nHEAD cccc\ndetached\nprunable gitdir file points to non-existent location\n";
        let list = parse_porcelain(text);
        assert_eq!(list.len(), 3);
        assert!(list[0].is_main && !list[1].is_main);
        assert_eq!(list[1].branch.as_deref(), Some("feat/x"));
        assert_eq!(list[1].locked.as_deref(), Some("in use"));
        assert!(list[2].detached && list[2].branch.is_none());
        assert!(list[2].prunable.is_some());
    }

    #[test]
    fn a_lock_without_reason_is_still_a_lock() {
        let list = parse_porcelain("worktree /a\nHEAD x\nbranch refs/heads/m\nlocked\n");
        assert_eq!(list[0].locked.as_deref(), Some(""));
    }
}
