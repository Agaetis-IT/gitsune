//! Branches, checkout and tags.

use serde::{Deserialize, Serialize};

use super::cli::{ErrorKind, GitError, RunOptions};
use super::ops::{OpCtx, Snapshot, bookkeeping_identity_env, check_paths, identity, take_snapshot};
use super::status::{FileState, WorkingStatus};

/// Where a checkout goes.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CheckoutTarget {
    /// A local branch.
    Branch { name: String },
    /// A remote-tracking branch (`origin/feature/x`): the local branch is created if needed.
    Remote { name: String },
    /// A commit, as a detached HEAD.
    Commit { id: String },
    /// A tag, as a detached HEAD.
    Tag { name: String },
}

impl CheckoutTarget {
    fn label(&self) -> &str {
        match self {
            Self::Branch { name } | Self::Remote { name } | Self::Tag { name } => name,
            Self::Commit { id } => id,
        }
    }
}

/// What to do with local changes that would be overwritten by a checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DirtyStrategy {
    /// Put the changes in a stash, switch, then apply the stash again.
    Stash,
    /// Throw the changes away (a snapshot is kept first).
    Discard,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutResult {
    pub status: WorkingStatus,
    /// Made before discarding changes; restoring it brings them back.
    pub snapshot: Option<Snapshot>,
    /// The changes were stashed and applied again after the switch.
    pub stashed: bool,
    /// Applying the stash again conflicted: the changes are still in the stash.
    pub stash_conflict: bool,
}

fn invalid(kind: &str, name: &str) -> GitError {
    GitError::new(
        ErrorKind::InvalidName,
        format!("'{name}' is not a valid {kind} name."),
    )
}

/// Refuses names git would not accept, before anything is run.
fn validate_name(ctx: &OpCtx, branch: bool, name: &str) -> Result<(), GitError> {
    let kind = if branch { "branch" } else { "tag" };
    if name.trim().is_empty() || name.trim() != name || name.starts_with('-') {
        return Err(invalid(kind, name));
    }
    let args: Vec<&str> = if branch {
        vec!["check-ref-format", "--branch", name]
    } else {
        let full = format!("refs/tags/{name}");
        return validate_full(ctx, &full).map_err(|_| invalid(kind, name));
    };
    let output = ctx
        .runner()
        .run_raw(ctx.workdir, &args, RunOptions::background())?;
    if output.code != 0 {
        return Err(invalid(kind, name));
    }
    Ok(())
}

fn validate_full(ctx: &OpCtx, full: &str) -> Result<(), GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["check-ref-format", full],
        RunOptions::background(),
    )?;
    if output.code != 0 {
        return Err(GitError::other("invalid name"));
    }
    Ok(())
}

fn ref_exists(ctx: &OpCtx, full: &str) -> Result<bool, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["show-ref", "--verify", "--quiet", full],
        RunOptions::background(),
    )?;
    Ok(output.code == 0)
}

fn resolve(ctx: &OpCtx, spec: &str) -> Result<String, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{spec}^{{commit}}"),
        ],
        RunOptions::background(),
    )?;
    if output.code != 0 {
        return Err(GitError::other(format!(
            "'{spec}' is not a commit of this repository."
        )));
    }
    Ok(output.stdout_text().trim().to_string())
}

fn already_exists(kind: &str, name: &str) -> GitError {
    GitError::new(
        ErrorKind::AlreadyExists,
        format!("A {kind} named '{name}' already exists."),
    )
}

/// Creates a branch at `start` (HEAD by default) and optionally switches to it.
pub fn create_branch(
    ctx: &OpCtx,
    name: &str,
    start: Option<&str>,
    checkout: bool,
) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, true, name)?;
    let _write = ctx.stores.begin_write()?;
    if ref_exists(ctx, &format!("refs/heads/{name}"))? {
        return Err(already_exists("branch", name));
    }
    let start = match start {
        Some(spec) => Some(resolve(ctx, spec)?),
        None => None,
    };
    let mut args = vec![if checkout { "switch" } else { "branch" }];
    if checkout {
        args.push("-c");
    }
    args.push(name);
    if let Some(start) = start.as_deref() {
        args.push(start);
    }
    ctx.runner()
        .run(ctx.workdir, &args, RunOptions::default())?;
    ctx.status()
}

/// Whether anything differs from HEAD: changes, untracked files or conflicts.
fn is_dirty(status: &WorkingStatus) -> bool {
    !status.staged.is_empty() || !status.unstaged.is_empty() || !status.conflicted.is_empty()
}

fn switch_args(ctx: &OpCtx, target: &CheckoutTarget) -> Result<Vec<String>, GitError> {
    Ok(match target {
        CheckoutTarget::Branch { name } => {
            if !ref_exists(ctx, &format!("refs/heads/{name}"))? {
                return Err(GitError::other(format!(
                    "There is no branch named '{name}'."
                )));
            }
            vec!["switch".into(), name.clone()]
        }
        CheckoutTarget::Remote { name } => {
            if !ref_exists(ctx, &format!("refs/remotes/{name}"))? {
                return Err(GitError::other(format!(
                    "There is no remote branch named '{name}'."
                )));
            }
            let local = name.split_once('/').map_or(name.as_str(), |(_, rest)| rest);
            if ref_exists(ctx, &format!("refs/heads/{local}"))? {
                vec!["switch".into(), local.to_string()]
            } else {
                vec![
                    "switch".into(),
                    "-c".into(),
                    local.to_string(),
                    "--track".into(),
                    name.clone(),
                ]
            }
        }
        CheckoutTarget::Commit { id } => {
            vec!["switch".into(), "--detach".into(), resolve(ctx, id)?]
        }
        CheckoutTarget::Tag { name } => {
            if !ref_exists(ctx, &format!("refs/tags/{name}"))? {
                return Err(GitError::other(format!("There is no tag named '{name}'.")));
            }
            vec![
                "switch".into(),
                "--detach".into(),
                format!("refs/tags/{name}"),
            ]
        }
    })
}

/// The identity to give stashes when git has none configured (they are bookkeeping commits).
fn fallback_identity(ctx: &OpCtx) -> Vec<(String, String)> {
    let known = identity(ctx).unwrap_or_default();
    if known.name.is_some() && known.email.is_some() {
        Vec::new()
    } else {
        bookkeeping_identity_env()
    }
}

/// Switches to a branch, a commit or a tag.
///
/// git decides first: changes that do not touch the target come along. When it refuses
/// (`WouldOverwriteChanges`, with the files), the caller may ask again with a `strategy`:
/// stash the changes and apply them after the switch, or discard them (after a snapshot).
pub fn checkout(
    ctx: &OpCtx,
    target: &CheckoutTarget,
    strategy: Option<DirtyStrategy>,
) -> Result<CheckoutResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let args = switch_args(ctx, target)?;
    let run_switch = |extra: &[&str]| -> Result<(), GitError> {
        let mut full: Vec<&str> = vec![args[0].as_str()];
        full.extend(extra);
        full.extend(args[1..].iter().map(String::as_str));
        ctx.runner()
            .run(ctx.workdir, &full, RunOptions::default())
            .map(|_| ())
    };
    let mut result = CheckoutResult {
        status: WorkingStatus::default(),
        snapshot: None,
        stashed: false,
        stash_conflict: false,
    };

    match strategy {
        None => run_switch(&[])?,
        Some(DirtyStrategy::Discard) => {
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
                result.snapshot = Some(take_snapshot(ctx, &tracked, "switching branches")?);
            }
            run_switch(&["--discard-changes"])?;
        }
        Some(DirtyStrategy::Stash) => {
            let dirty = is_dirty(&ctx.status()?);
            if dirty {
                let message = format!("git-hydra: before switching to {}", target.label());
                let options = RunOptions::with_env(fallback_identity(ctx));
                ctx.runner().run(
                    ctx.workdir,
                    &["stash", "push", "--include-untracked", "-m", &message],
                    options,
                )?;
            }
            if let Err(error) = run_switch(&[]) {
                if dirty {
                    // Put everything back where it was before reporting the failure.
                    let _ = ctx.runner().run_raw(
                        ctx.workdir,
                        &["stash", "pop", "--index"],
                        RunOptions::default(),
                    );
                }
                return Err(error);
            }
            if dirty {
                let pop = ctx.runner().run_raw(
                    ctx.workdir,
                    &["stash", "pop", "--index"],
                    RunOptions::with_env(fallback_identity(ctx)),
                )?;
                if pop.code == 0 {
                    result.stashed = true;
                } else {
                    result.stash_conflict = true;
                }
            }
        }
    }
    result.status = ctx.status()?;
    Ok(result)
}

/// Renames a local branch.
pub fn rename_branch(ctx: &OpCtx, old: &str, new: &str) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, true, new)?;
    let _write = ctx.stores.begin_write()?;
    if !ref_exists(ctx, &format!("refs/heads/{old}"))? {
        return Err(GitError::other(format!(
            "There is no branch named '{old}'."
        )));
    }
    if old != new && ref_exists(ctx, &format!("refs/heads/{new}"))? {
        return Err(already_exists("branch", new));
    }
    ctx.runner().run(
        ctx.workdir,
        &["branch", "-m", old, new],
        RunOptions::default(),
    )?;
    ctx.status()
}

/// A branch that was deleted, with what is needed to bring it back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedBranch {
    pub name: String,
    /// The commit the branch pointed at.
    pub tip: String,
    pub upstream: Option<String>,
    pub status: WorkingStatus,
}

/// Deletes a local branch. Without `force`, git refuses a branch with commits that are not
/// merged anywhere else (`NotFullyMerged`); the caller can then confirm and ask again with `force`.
pub fn delete_branch(ctx: &OpCtx, name: &str, force: bool) -> Result<DeletedBranch, GitError> {
    check_paths(&[name.to_string()])?;
    let _write = ctx.stores.begin_write()?;
    let full = format!("refs/heads/{name}");
    if !ref_exists(ctx, &full)? {
        return Err(GitError::other(format!(
            "There is no branch named '{name}'."
        )));
    }
    let tip = ctx
        .runner()
        .run(ctx.workdir, &["rev-parse", &full], RunOptions::background())?
        .stdout_text()
        .trim()
        .to_string();
    let upstream = ctx
        .runner()
        .run(
            ctx.workdir,
            &["for-each-ref", "--format=%(upstream:short)", &full],
            RunOptions::background(),
        )?
        .stdout_text()
        .trim()
        .to_string();
    ctx.runner().run(
        ctx.workdir,
        &["branch", if force { "-D" } else { "-d" }, name],
        RunOptions::default(),
    )?;
    Ok(DeletedBranch {
        name: name.to_string(),
        tip,
        upstream: (!upstream.is_empty()).then_some(upstream),
        status: ctx.status()?,
    })
}

/// Brings back a deleted branch at its old tip, with its upstream when that still exists.
pub fn restore_branch(
    ctx: &OpCtx,
    name: &str,
    tip: &str,
    upstream: Option<&str>,
) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, true, name)?;
    let _write = ctx.stores.begin_write()?;
    if ref_exists(ctx, &format!("refs/heads/{name}"))? {
        return Err(already_exists("branch", name));
    }
    let tip = resolve(ctx, tip)?;
    ctx.runner()
        .run(ctx.workdir, &["branch", name, &tip], RunOptions::default())?;
    if let Some(upstream) = upstream {
        // The remote branch may be gone meanwhile: the branch is back either way.
        let _ = ctx.runner().run_raw(
            ctx.workdir,
            &["branch", &format!("--set-upstream-to={upstream}"), name],
            RunOptions::default(),
        );
    }
    ctx.status()
}

/// Makes a branch track `upstream` (`origin/main`), or stop tracking when it is `None`.
pub fn set_upstream(
    ctx: &OpCtx,
    branch: &str,
    upstream: Option<&str>,
) -> Result<WorkingStatus, GitError> {
    check_paths(&[branch.to_string()])?;
    let _write = ctx.stores.begin_write()?;
    if !ref_exists(ctx, &format!("refs/heads/{branch}"))? {
        return Err(GitError::other(format!(
            "There is no branch named '{branch}'."
        )));
    }
    match upstream {
        Some(upstream) => {
            let option = format!("--set-upstream-to={upstream}");
            ctx.runner().run(
                ctx.workdir,
                &["branch", &option, branch],
                RunOptions::default(),
            )?;
        }
        None => {
            ctx.runner().run(
                ctx.workdir,
                &["branch", "--unset-upstream", branch],
                RunOptions::default(),
            )?;
        }
    }
    ctx.status()
}

// ---------- tags ----------

/// Creates a tag at `target` (HEAD by default): annotated when a message is given.
pub fn create_tag(
    ctx: &OpCtx,
    name: &str,
    target: Option<&str>,
    message: Option<&str>,
) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, false, name)?;
    let _write = ctx.stores.begin_write()?;
    if ref_exists(ctx, &format!("refs/tags/{name}"))? {
        return Err(already_exists("tag", name));
    }
    let target = match target {
        Some(spec) => Some(resolve(ctx, spec)?),
        None => None,
    };
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    let mut args = vec!["tag"];
    if message.is_some() {
        args.extend(["-a", "-F", "-"]);
    }
    args.push(name);
    if let Some(target) = target.as_deref() {
        args.push(target);
    }
    let options = match message {
        Some(message) => {
            let mut options = RunOptions::with_stdin(format!("{message}\n").into_bytes());
            options.env = fallback_identity(ctx);
            options
        }
        None => RunOptions::default(),
    };
    ctx.runner().run(ctx.workdir, &args, options)?;
    ctx.status()
}

/// A tag that was deleted, with what is needed to bring it back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedTag {
    pub name: String,
    /// What the tag pointed at: the commit, or the tag object of an annotated tag.
    pub target: String,
    pub status: WorkingStatus,
}

pub fn delete_tag(ctx: &OpCtx, name: &str) -> Result<DeletedTag, GitError> {
    check_paths(&[name.to_string()])?;
    let _write = ctx.stores.begin_write()?;
    let full = format!("refs/tags/{name}");
    if !ref_exists(ctx, &full)? {
        return Err(GitError::other(format!("There is no tag named '{name}'.")));
    }
    let target = ctx
        .runner()
        .run(ctx.workdir, &["rev-parse", &full], RunOptions::background())?
        .stdout_text()
        .trim()
        .to_string();
    ctx.runner()
        .run(ctx.workdir, &["tag", "-d", name], RunOptions::default())?;
    Ok(DeletedTag {
        name: name.to_string(),
        target,
        status: ctx.status()?,
    })
}

/// Brings back a deleted tag (lightweight or annotated) exactly as it was.
pub fn restore_tag(ctx: &OpCtx, name: &str, target: &str) -> Result<WorkingStatus, GitError> {
    validate_name(ctx, false, name)?;
    let _write = ctx.stores.begin_write()?;
    let full = format!("refs/tags/{name}");
    if ref_exists(ctx, &full)? {
        return Err(already_exists("tag", name));
    }
    ctx.runner().run(
        ctx.workdir,
        &["update-ref", &full, target],
        RunOptions::default(),
    )?;
    ctx.status()
}
