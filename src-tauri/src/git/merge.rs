//! Merging, and getting out of an operation that stopped on conflicts.
//!
//! A merge that conflicts is **left in progress** (that is what the interface resolves): the
//! status then says so (`operation`) and lists the conflicted files. [`conflict_file`] gives
//! the three versions of one, [`resolve_file`] and [`resolve_side`] settle it, and
//! [`continue_operation`] or [`abort_operation`] finish or cancel the whole operation.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use super::branches::{fallback_identity, resolve};
use super::cli::{ErrorKind, GitError, RunOptions, error_from};
use super::ops::{OpCtx, head_id};
use super::status::{OperationKind, WorkingStatus};

/// How a merge is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeMode {
    /// Fast-forward when the branch can simply move, a merge commit otherwise.
    Default,
    /// Always a merge commit, even when a fast-forward is possible.
    NoFastForward,
    /// Only fast-forward; refuse when the branches diverged.
    FastForwardOnly,
    /// Stage the combined changes without committing and without recording the merge.
    Squash,
}

/// What merging a branch would do, to explain it before asking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeInfo {
    /// The target is already part of the current branch: nothing to merge.
    pub up_to_date: bool,
    /// The current branch can simply move to the target.
    pub can_fast_forward: bool,
    /// Commits of the target that the current branch does not have.
    pub commits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOutcome {
    UpToDate,
    FastForward,
    Merged,
    /// The changes are staged; the user commits them.
    Squashed,
    /// The merge stopped: the files with conflicts are in the status.
    Conflicts,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub status: WorkingStatus,
    pub outcome: MergeOutcome,
    /// HEAD before and after: what Undo needs (a merge that stopped has not moved HEAD).
    pub before: String,
    pub after: String,
}

fn check_target(target: &str) -> Result<(), GitError> {
    if target.trim().is_empty() || target.starts_with('-') || target.contains('\0') {
        return Err(GitError::other(format!("'{target}' cannot be merged.")));
    }
    Ok(())
}

fn is_ancestor(ctx: &OpCtx, ancestor: &str, of: &str) -> Result<bool, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["merge-base", "--is-ancestor", ancestor, of],
        RunOptions::background(),
    )?;
    Ok(output.code == 0)
}

pub fn merge_info(ctx: &OpCtx, target: &str) -> Result<MergeInfo, GitError> {
    check_target(target)?;
    let id = resolve(ctx, target)?;
    let head = head_id(ctx)?.ok_or_else(|| GitError::other("There is no commit yet."))?;
    let up_to_date = is_ancestor(ctx, &id, &head)?;
    let can_fast_forward = !up_to_date && is_ancestor(ctx, &head, &id)?;
    let count = ctx.runner().run(
        ctx.workdir,
        &["rev-list", "--count", &format!("{head}..{id}")],
        RunOptions::background(),
    )?;
    Ok(MergeInfo {
        up_to_date,
        can_fast_forward,
        commits: count.stdout_text().trim().parse().unwrap_or(0),
    })
}

fn refuse_if_busy(status: &WorkingStatus) -> Result<(), GitError> {
    if status.operation.is_some() || !status.conflicted.is_empty() {
        return Err(GitError::other(
            "Another operation is waiting to be finished or cancelled: do that first.",
        ));
    }
    Ok(())
}

/// Merges `target` (a branch, a remote branch or a commit) into the current branch.
pub fn merge(
    ctx: &OpCtx,
    target: &str,
    mode: MergeMode,
    message: Option<&str>,
) -> Result<MergeResult, GitError> {
    check_target(target)?;
    let _write = ctx.stores.begin_write()?;
    refuse_if_busy(&ctx.status()?)?;
    resolve(ctx, target)?;
    let before = head_id(ctx)?.ok_or_else(|| GitError::other("There is no commit yet."))?;

    let mut args = vec!["merge", "--no-edit"];
    match mode {
        MergeMode::Default => {}
        MergeMode::NoFastForward => args.push("--no-ff"),
        MergeMode::FastForwardOnly => args.push("--ff-only"),
        MergeMode::Squash => args.push("--squash"),
    }
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    if let Some(message) = message {
        args.push("-m");
        args.push(message);
    }
    args.push(target);

    let output = ctx.runner().run_raw(
        ctx.workdir,
        &args,
        RunOptions::with_env(fallback_identity(ctx)),
    )?;
    let status = ctx.status()?;
    if output.code != 0 {
        if !status.conflicted.is_empty() {
            return Ok(MergeResult {
                status,
                outcome: MergeOutcome::Conflicts,
                before: before.clone(),
                after: before,
            });
        }
        return Err(error_from(&output));
    }
    let after = head_id(ctx)?.unwrap_or_else(|| before.clone());
    let said = output.stdout_text();
    let outcome = if said.contains("Already up to date") {
        MergeOutcome::UpToDate
    } else if mode == MergeMode::Squash {
        MergeOutcome::Squashed
    } else if said.contains("Fast-forward") {
        MergeOutcome::FastForward
    } else {
        MergeOutcome::Merged
    };
    Ok(MergeResult {
        status,
        outcome,
        before,
        after,
    })
}

// ---------- finishing or cancelling ----------

/// Finishes the operation in progress once every conflict is resolved: commits the merge, or
/// goes on with the cherry-pick, the revert or the rebase.
pub fn continue_operation(ctx: &OpCtx) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let status = ctx.status()?;
    if !status.conflicted.is_empty() {
        let files: Vec<String> = status.conflicted.iter().map(|c| c.path.clone()).collect();
        return Err(GitError::new(
            ErrorKind::Conflict,
            format!("{} file(s) still have conflicts.", files.len()),
        )
        .with_files(files));
    }
    let args: Vec<&str> = match status.operation.as_ref().map(|op| op.kind) {
        Some(OperationKind::Merge) => vec!["commit", "--no-edit"],
        // Conflicts without an operation (a squash, a stash): the user commits as usual.
        None => return Err(GitError::other("There is no operation to continue.")),
        Some(OperationKind::CherryPick) => vec!["cherry-pick", "--continue"],
        Some(OperationKind::Revert) => vec!["revert", "--continue"],
        Some(OperationKind::Rebase) => vec!["rebase", "--continue"],
    };
    let mut env = fallback_identity(ctx);
    env.extend(super::rebase::rebase_env(ctx)?);
    if status.operation.as_ref().map(|op| op.kind) == Some(OperationKind::Rebase) {
        super::rebase::commit_chosen_reword(ctx)?;
    }
    let output = ctx
        .runner()
        .run_raw(ctx.workdir, &args, RunOptions::with_env(env))?;
    let after = ctx.status()?;
    if output.code != 0 {
        // A rebase or a cherry-pick of several commits stops again at the next conflict: that
        // is the operation going on, not a failure.
        if !after.conflicted.is_empty() && after.operation.is_some() {
            return Ok(after);
        }
        return Err(error_from(&output));
    }
    Ok(after)
}

/// Skips the commit the operation is stuck on (a rebase or a cherry-pick): its changes are
/// left out.
pub fn skip_operation(ctx: &OpCtx) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let status = ctx.status()?;
    let args: Vec<&str> = match status.operation.as_ref().map(|op| op.kind) {
        Some(OperationKind::Rebase) => vec!["rebase", "--skip"],
        Some(OperationKind::CherryPick) => vec!["cherry-pick", "--skip"],
        _ => {
            return Err(GitError::other(
                "Only a rebase or a cherry-pick can be skipped.",
            ));
        }
    };
    let mut env = fallback_identity(ctx);
    env.extend(super::rebase::rebase_env(ctx)?);
    let output = ctx
        .runner()
        .run_raw(ctx.workdir, &args, RunOptions::with_env(env))?;
    let after = ctx.status()?;
    if output.code != 0 && (after.operation.is_none() || after.conflicted.is_empty()) {
        return Err(error_from(&output));
    }
    Ok(after)
}

/// Cancels the operation in progress and puts the repository back where it was before it.
pub fn abort_operation(ctx: &OpCtx) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    let status = ctx.status()?;
    let args: Vec<&str> = match status.operation.as_ref().map(|op| op.kind) {
        Some(OperationKind::Merge) => vec!["merge", "--abort"],
        Some(OperationKind::CherryPick) => vec!["cherry-pick", "--abort"],
        Some(OperationKind::Revert) => vec!["revert", "--abort"],
        Some(OperationKind::Rebase) => vec!["rebase", "--abort"],
        // A squash merge that conflicted leaves no operation behind, only conflicts.
        None if !status.conflicted.is_empty() => vec!["reset", "--merge"],
        None => return Err(GitError::other("There is nothing to cancel.")),
    };
    ctx.runner()
        .run(ctx.workdir, &args, RunOptions::default())?;
    ctx.status()
}

// ---------- resolving one file ----------

/// Files above this size are resolved by choosing a side, not edited in the app.
const MAX_EDIT_BYTES: usize = 2 * 1024 * 1024;

/// The three versions of a conflicted file and what is in the working tree now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    /// The two-letter code of git (`UU` both modified, `DU` deleted by us...).
    pub code: String,
    /// Our version (the current branch); `None` when we deleted the file.
    pub ours: Option<String>,
    /// Their version (what is merged in); `None` when they deleted it.
    pub theirs: Option<String>,
    /// The common ancestor; `None` when the file is new on both sides.
    pub base: Option<String>,
    /// The file as it is in the working tree: with conflict markers, or already edited.
    pub worktree: Option<String>,
    /// Binary or too large: only "ours" or "theirs" can be chosen.
    pub choose_side_only: bool,
}

/// The conflicted file `path` of the status; anything else is refused, so nothing outside the
/// conflicts can be rewritten through these commands.
fn conflicted_code(ctx: &OpCtx, path: &str) -> Result<String, GitError> {
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(GitError::other("invalid path"));
    }
    ctx.status()?
        .conflicted
        .into_iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.code)
        .ok_or_else(|| GitError::other(format!("{path} has no conflict (any more).")))
}

fn stage(ctx: &OpCtx, number: u8, path: &str) -> Result<Option<Vec<u8>>, GitError> {
    ctx.blob(&format!(":{number}:{path}"))
}

pub fn conflict_file(ctx: &OpCtx, path: &str) -> Result<ConflictFile, GitError> {
    let code = conflicted_code(ctx, path)?;
    let (base, ours, theirs) = (
        stage(ctx, 1, path)?,
        stage(ctx, 2, path)?,
        stage(ctx, 3, path)?,
    );
    let worktree = std::fs::read(ctx.workdir.join(path)).ok();
    let versions = [&base, &ours, &theirs, &worktree];
    let binary = versions
        .iter()
        .any(|v| v.as_ref().is_some_and(|bytes| bytes.contains(&0)));
    let too_large = versions
        .iter()
        .any(|v| v.as_ref().is_some_and(|bytes| bytes.len() > MAX_EDIT_BYTES));
    let text = |bytes: Option<Vec<u8>>| {
        bytes
            .filter(|_| !binary && !too_large)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    Ok(ConflictFile {
        path: path.to_string(),
        code,
        ours: text(ours),
        theirs: text(theirs),
        base: text(base),
        worktree: text(worktree),
        choose_side_only: binary || too_large,
    })
}

/// Writes the resolved content of a conflicted file and marks it resolved (`git add`).
pub fn resolve_file(ctx: &OpCtx, path: &str, content: &str) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    conflicted_code(ctx, path)?;
    std::fs::write(ctx.workdir.join(path), content)
        .map_err(|e| GitError::other(format!("cannot write {path}: {e}")))?;
    ctx.runner()
        .run(ctx.workdir, &["add", "--", path], RunOptions::default())?;
    ctx.status()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Ours,
    Theirs,
}

/// Resolves a conflicted file by taking one side as it is. When that side deleted the file the
/// file is deleted.
pub fn resolve_side(ctx: &OpCtx, path: &str, side: Side) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    conflicted_code(ctx, path)?;
    let (number, flag) = match side {
        Side::Ours => (2, "--ours"),
        Side::Theirs => (3, "--theirs"),
    };
    if stage(ctx, number, path)?.is_some() {
        ctx.runner().run(
            ctx.workdir,
            &["checkout", flag, "--", path],
            RunOptions::default(),
        )?;
        ctx.runner()
            .run(ctx.workdir, &["add", "--", path], RunOptions::default())?;
    } else {
        ctx.runner().run(
            ctx.workdir,
            &["rm", "-q", "--", path],
            RunOptions::default(),
        )?;
    }
    ctx.status()
}
