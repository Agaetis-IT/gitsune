//! The stash: list, create, apply, pop, drop (with a way back).

use serde::Serialize;

use super::branches::fallback_identity;
use super::cli::{ErrorKind, GitError, RunOptions};
use super::ops::OpCtx;
use super::status::WorkingStatus;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// Position in the list (0 is the most recent). It shifts when a stash is dropped.
    pub index: usize,
    /// `stash@{n}`.
    pub reference: String,
    pub id: String,
    /// What the user (or the app) said, or git's own `WIP on main: ...`.
    pub message: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

/// A stash that was dropped, with what is needed to bring it back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedStash {
    pub id: String,
    pub message: String,
    pub status: WorkingStatus,
}

pub fn list_stashes(ctx: &OpCtx) -> Result<Vec<StashEntry>, GitError> {
    let output = ctx.runner().run(
        ctx.workdir,
        &["stash", "list", "--format=%H%x1f%ct%x1f%gs%x1e"],
        RunOptions::background(),
    )?;
    let text = output.stdout_text();
    Ok(text
        .split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .enumerate()
        .filter_map(|(index, record)| {
            let mut parts = record.splitn(3, '\x1f');
            let id = parts.next()?.to_string();
            let time = parts.next()?.parse().ok()?;
            let message = parts.next()?.trim().to_string();
            Some(StashEntry {
                index,
                reference: format!("stash@{{{index}}}"),
                id,
                message,
                time,
            })
        })
        .collect())
}

/// The stash the user chose, checked against the list: the position of a stash shifts when
/// another one is dropped, so a stale reference must never act on the wrong one.
fn find(ctx: &OpCtx, reference: &str, id: &str) -> Result<StashEntry, GitError> {
    list_stashes(ctx)?
        .into_iter()
        .find(|entry| entry.reference == reference && entry.id == id)
        .ok_or_else(|| GitError::other("The list of stashes changed: reload it and try again."))
}

/// Puts the changes of the working tree in a new stash and cleans the tree.
pub fn stash_push(
    ctx: &OpCtx,
    message: Option<&str>,
    include_untracked: bool,
    keep_index: bool,
) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    if keep_index {
        args.push("--keep-index");
    }
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    if let Some(message) = message {
        args.push("-m");
        args.push(message);
    }
    let output = ctx.runner().run(
        ctx.workdir,
        &args,
        RunOptions::with_env(fallback_identity(ctx)),
    )?;
    if output.stdout_text().contains("No local changes to save") {
        return Err(GitError::new(
            ErrorKind::NothingToCommit,
            "There is nothing to stash.",
        ));
    }
    ctx.status()
}

fn apply_or_pop(
    ctx: &OpCtx,
    verb: &str,
    reference: &str,
    id: &str,
    restore_index: bool,
) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let entry = find(ctx, reference, id)?;
    let mut args = vec!["stash", verb];
    if restore_index {
        args.push("--index");
    }
    args.push(&entry.reference);
    ctx.runner().run(
        ctx.workdir,
        &args,
        RunOptions::with_env(fallback_identity(ctx)),
    )?;
    ctx.status()
}

/// Applies a stash and keeps it in the list.
pub fn stash_apply(
    ctx: &OpCtx,
    reference: &str,
    id: &str,
    restore_index: bool,
) -> Result<WorkingStatus, GitError> {
    apply_or_pop(ctx, "apply", reference, id, restore_index)
}

/// Applies a stash and removes it from the list when that worked (a conflict keeps it).
pub fn stash_pop(
    ctx: &OpCtx,
    reference: &str,
    id: &str,
    restore_index: bool,
) -> Result<WorkingStatus, GitError> {
    apply_or_pop(ctx, "pop", reference, id, restore_index)
}

/// Removes a stash from the list. Its commit stays in the repository for a while, which is
/// what [`stash_restore`] uses to bring it back.
pub fn stash_drop(ctx: &OpCtx, reference: &str, id: &str) -> Result<DroppedStash, GitError> {
    let _write = ctx.stores.begin_write()?;
    let entry = find(ctx, reference, id)?;
    ctx.runner().run(
        ctx.workdir,
        &["stash", "drop", &entry.reference],
        RunOptions::default(),
    )?;
    Ok(DroppedStash {
        id: entry.id,
        message: entry.message,
        status: ctx.status()?,
    })
}

/// Puts a dropped stash back at the top of the list.
pub fn stash_restore(ctx: &OpCtx, id: &str, message: &str) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let exists = ctx.runner().run_raw(
        ctx.workdir,
        &["cat-file", "-e", &format!("{id}^{{commit}}")],
        RunOptions::background(),
    )?;
    if exists.code != 0 {
        return Err(GitError::other("That stash no longer exists."));
    }
    ctx.runner().run(
        ctx.workdir,
        &["stash", "store", "-m", message, id],
        RunOptions::default(),
    )?;
    ctx.status()
}
