//! Cherry-pick, revert and reset, each with a way back.
//!
//! A cherry-pick or a revert that stops on conflicts is cancelled and reported: resolving
//! conflicts comes with the merge milestone, and the repository is never left half-way.

use serde::{Deserialize, Serialize};

use super::branches::{fallback_identity, resolve, validate_name};
use super::cli::{ErrorKind, GitError, RunOptions};
use super::ops::{DiscardResult, OpCtx, Snapshot, head_id, restore_snapshot, take_snapshot};
use super::status::{FileState, WorkingStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOpResult {
    pub status: WorkingStatus,
    /// HEAD before and after: what Undo needs.
    pub before: String,
    pub after: String,
}

fn is_merge(ctx: &OpCtx, id: &str) -> Result<bool, GitError> {
    let output = ctx.runner().run(
        ctx.workdir,
        &["rev-list", "--parents", "-n", "1", id],
        RunOptions::background(),
    )?;
    Ok(output.stdout_text().split_whitespace().count() > 2)
}

/// Whether the repository holds that pseudo-reference (`CHERRY_PICK_HEAD`...): an operation
/// is waiting for the user.
fn in_progress(ctx: &OpCtx, pseudo_ref: &str) -> Result<bool, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["rev-parse", "-q", "--verify", pseudo_ref],
        RunOptions::background(),
    )?;
    Ok(output.code == 0)
}

/// Replays or reverts one commit on top of HEAD.
fn replay(ctx: &OpCtx, verb: &str, spec: &str) -> Result<CommitOpResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let id = resolve(ctx, spec)?;
    let before =
        head_id(ctx)?.ok_or_else(|| GitError::other("There is no commit yet to build on."))?;
    let head_ref = if verb == "revert" {
        "REVERT_HEAD"
    } else {
        "CHERRY_PICK_HEAD"
    };
    for other in ["CHERRY_PICK_HEAD", "REVERT_HEAD", "MERGE_HEAD"] {
        if in_progress(ctx, other)? {
            return Err(GitError::other(
                "Another operation (merge, cherry-pick or revert) is already in progress in this repository.",
            ));
        }
    }
    let mut args = vec![verb];
    if verb == "revert" {
        args.push("--no-edit");
    }
    // A merge commit has several sides: follow the first parent (the branch merged into).
    if is_merge(ctx, &id)? {
        args.extend(["-m", "1"]);
    }
    args.push(&id);
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &args,
        RunOptions::with_env(fallback_identity(ctx)),
    )?;
    if output.code != 0 {
        let error = super::cli::error_from(&output);
        let text = output.failure_text();
        let empty = text.contains("now empty") || text.contains("nothing to commit");
        // Leave nothing half-done: an operation that stopped is cancelled.
        if in_progress(ctx, head_ref)? {
            let _ = ctx
                .runner()
                .run_raw(ctx.workdir, &[verb, "--abort"], RunOptions::default());
        }
        return Err(match error.kind {
            _ if empty => GitError::new(
                ErrorKind::NothingToCommit,
                "Those changes are already in this branch: nothing to do.",
            ),
            ErrorKind::Conflict => GitError::new(
                ErrorKind::Conflict,
                format!(
                    "The {verb} conflicts with the current branch, so it was cancelled and nothing changed. Resolving conflicts comes in a later version."
                ),
            ),
            _ => error,
        });
    }
    let after = head_id(ctx)?.unwrap_or_else(|| before.clone());
    Ok(CommitOpResult {
        status: ctx.status()?,
        before,
        after,
    })
}

pub fn cherry_pick(ctx: &OpCtx, commit: &str) -> Result<CommitOpResult, GitError> {
    replay(ctx, "cherry-pick", commit)
}

pub fn revert_commit(ctx: &OpCtx, commit: &str) -> Result<CommitOpResult, GitError> {
    replay(ctx, "revert", commit)
}

/// Takes back a cherry-pick or a revert: the branch goes back to `before`, unless HEAD moved
/// since or local changes would be lost (`reset --keep` refuses those).
pub fn undo_replay(ctx: &OpCtx, before: &str, after: &str) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    if head_id(ctx)?.as_deref() != Some(after) {
        return Err(GitError::other(
            "HEAD has moved since then: this cannot be undone from here.",
        ));
    }
    ctx.runner().run(
        ctx.workdir,
        &["reset", "--keep", before],
        RunOptions::default(),
    )?;
    ctx.status()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    /// Move the branch only: the changes stay staged.
    Soft,
    /// Move the branch and unstage: the changes stay in the files.
    Mixed,
    /// Move the branch and make the files match: uncommitted changes are discarded.
    Hard,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetResult {
    pub status: WorkingStatus,
    pub before: String,
    pub after: String,
    /// Made before a hard reset threw uncommitted changes away.
    pub snapshot: Option<Snapshot>,
}

/// Moves the current branch (or the detached HEAD) to `target`.
pub fn reset(ctx: &OpCtx, target: &str, mode: ResetMode) -> Result<ResetResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let id = resolve(ctx, target)?;
    let before = head_id(ctx)?.ok_or_else(|| GitError::other("There is no commit yet to move."))?;
    let mut snapshot = None;
    if mode == ResetMode::Hard {
        let status = ctx.status()?;
        let tracked: Vec<String> = status
            .staged
            .iter()
            .chain(&status.unstaged)
            .filter(|entry| entry.state != FileState::Untracked)
            .map(|entry| entry.path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if !tracked.is_empty() {
            snapshot = Some(take_snapshot(ctx, &tracked, "reset --hard")?);
        }
    }
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    ctx.runner()
        .run(ctx.workdir, &["reset", flag, &id], RunOptions::default())?;
    Ok(ResetResult {
        status: ctx.status()?,
        before,
        after: id,
        snapshot,
    })
}

/// The result of moving a branch, which may not be the checked-out one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchResetResult {
    pub status: WorkingStatus,
    pub branch: String,
    /// Where the branch was; `None` when this call created it.
    pub before: Option<String>,
    pub after: String,
    /// The checked-out branch: the reset went through HEAD, and `undo_reset` takes it back.
    pub current: bool,
    /// Made before a hard reset threw uncommitted changes away.
    pub snapshot: Option<Snapshot>,
}

/// The checked-out branch, `None` when HEAD is detached.
fn current_branch(ctx: &OpCtx) -> Result<Option<String>, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["symbolic-ref", "-q", "--short", "HEAD"],
        RunOptions::background(),
    )?;
    Ok((output.code == 0).then(|| output.stdout_text().trim().to_string()))
}

/// The commit a branch points at, `None` when it does not exist.
fn branch_tip(ctx: &OpCtx, full: &str) -> Result<Option<String>, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["rev-parse", "--verify", "-q", full],
        RunOptions::background(),
    )?;
    Ok((output.code == 0).then(|| output.stdout_text().trim().to_string()))
}

/// Moves any local branch to `target`. The checked-out branch goes through `reset`. Another
/// branch only has its ref moved: soft and mixed change nothing else, and hard is refused since
/// it would rewrite the files of the checked-out branch. A branch that does not exist is created
/// at `target` (a remote-tracking `target` also sets its upstream).
pub fn reset_branch(
    ctx: &OpCtx,
    branch: &str,
    target: &str,
    mode: ResetMode,
) -> Result<BranchResetResult, GitError> {
    validate_name(ctx, true, branch)?;
    if current_branch(ctx)?.as_deref() == Some(branch) {
        let result = reset(ctx, target, mode)?;
        return Ok(BranchResetResult {
            status: result.status,
            branch: branch.to_string(),
            before: Some(result.before),
            after: result.after,
            current: true,
            snapshot: result.snapshot,
        });
    }
    if mode == ResetMode::Hard {
        return Err(GitError::other(format!(
            "A hard reset rewrites the files, so '{branch}' must be checked out first. Use soft or mixed, or check it out."
        )));
    }
    let _write = ctx.stores.begin_write()?;
    let id = resolve(ctx, target)?;
    let full = format!("refs/heads/{branch}");
    let Some(before) = branch_tip(ctx, &full)? else {
        ctx.runner().run(
            ctx.workdir,
            &["branch", branch, target],
            RunOptions::default(),
        )?;
        return Ok(BranchResetResult {
            status: ctx.status()?,
            branch: branch.to_string(),
            before: None,
            after: id,
            current: false,
            snapshot: None,
        });
    };
    // The old value makes git refuse the move if the branch changed in the meantime.
    ctx.runner().run(
        ctx.workdir,
        &[
            "update-ref",
            "-m",
            &format!("reset: moving to {target}"),
            &full,
            &id,
            &before,
        ],
        RunOptions::default(),
    )?;
    Ok(BranchResetResult {
        status: ctx.status()?,
        branch: branch.to_string(),
        before: Some(before),
        after: id,
        current: false,
        snapshot: None,
    })
}

/// Takes back `reset_branch` on a branch that is not checked out: the branch goes back to
/// `before`, or is deleted when this reset created it. Refused if the branch moved since.
pub fn undo_branch_reset(
    ctx: &OpCtx,
    branch: &str,
    before: Option<&str>,
    after: &str,
) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, true, branch)?;
    let _write = ctx.stores.begin_write()?;
    let full = format!("refs/heads/{branch}");
    if branch_tip(ctx, &full)?.as_deref() != Some(after) {
        return Err(GitError::other(format!(
            "'{branch}' has moved since then: this cannot be undone from here."
        )));
    }
    match before {
        Some(before) => ctx.runner().run(
            ctx.workdir,
            &["update-ref", "-m", "reset: undo", &full, before, after],
            RunOptions::default(),
        )?,
        None => ctx.runner().run(
            ctx.workdir,
            &["update-ref", "-d", &full, after],
            RunOptions::default(),
        )?,
    };
    ctx.status()
}

/// Takes back a reset: the branch goes back to `before` and, after a hard reset, the
/// discarded changes come back from the snapshot. Refused if HEAD moved since.
pub fn undo_reset(
    ctx: &OpCtx,
    before: &str,
    after: &str,
    snapshot: Option<&str>,
) -> Result<DiscardResult, GitError> {
    {
        let _write = ctx.stores.begin_write()?;
        if head_id(ctx)?.as_deref() != Some(after) {
            return Err(GitError::other(
                "HEAD has moved since then: this cannot be undone from here.",
            ));
        }
        // `--keep` refuses to overwrite changes made since, and keeps what does not clash.
        ctx.runner().run(
            ctx.workdir,
            &["reset", "--keep", before],
            RunOptions::default(),
        )?;
    }
    match snapshot {
        Some(reference) => restore_snapshot(ctx, reference),
        None => Ok(DiscardResult {
            snapshot: None,
            status: ctx.status()?,
        }),
    }
}
