//! Fetch, pull, push and deleting a remote branch.
//!
//! These commands can take a while and talk to the network, so they run through
//! [`Runner::run_long`]: progress is streamed, secrets are asked through the askpass helper,
//! and the user can cancel. Authentication is the system's own (credential helpers, ssh-agent).

use serde::{Deserialize, Serialize};

use super::cli::{ErrorKind, GitError, LongIo, RunOptions, RunOutput};
use super::ops::OpCtx;
use super::status::WorkingStatus;

/// How a pull reconciles the remote branch with the local one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PullStrategy {
    /// Only move forward; refuse when the branches diverged.
    FastForwardOnly,
    /// Make a merge commit when needed.
    Merge,
    /// Replay the local commits on top of the remote ones.
    Rebase,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteResult {
    pub status: WorkingStatus,
    /// What git said once done (`Already up to date.`, the refs updated...).
    pub summary: String,
}

/// A name that is passed to git as an argument must not look like an option.
fn check_name(kind: &str, name: &str) -> Result<(), GitError> {
    if name.trim().is_empty() || name.starts_with('-') || name.contains('\0') {
        return Err(GitError::new(
            ErrorKind::InvalidName,
            format!("'{name}' is not a valid {kind} name."),
        ));
    }
    Ok(())
}

/// The last lines git printed, without the progress: what the user wants to read afterwards.
fn summarize(output: &RunOutput) -> String {
    let text = format!("{}\n{}", output.stderr, output.stdout_text());
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(8);
    lines[start..].join("\n")
}

/// The names of the remotes of the repository.
pub fn list_remotes(ctx: &OpCtx) -> Result<Vec<String>, GitError> {
    let output = ctx
        .runner()
        .run(ctx.workdir, &["remote"], RunOptions::background())?;
    Ok(output
        .stdout_text()
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

fn finish(ctx: &OpCtx, output: RunOutput) -> Result<RemoteResult, GitError> {
    Ok(RemoteResult {
        summary: summarize(&output),
        status: ctx.status()?,
    })
}

/// Downloads new commits and refs. Only remote-tracking refs change (never a branch, the index
/// or the working tree), so it is allowed in read-only mode and does not wait for other writes.
pub fn fetch(
    ctx: &OpCtx,
    io: &LongIo,
    remote: Option<&str>,
    prune: bool,
) -> Result<RemoteResult, GitError> {
    let mut args = vec!["fetch", "--progress"];
    if prune {
        args.push("--prune");
    }
    match remote {
        Some(name) => {
            check_name("remote", name)?;
            args.push(name);
        }
        None => args.push("--all"),
    }
    let output = ctx
        .runner()
        .run_long_checked(ctx.workdir, &args, RunOptions::default(), io)?;
    finish(ctx, output)
}

/// Fetches and integrates the upstream of the current branch. Without a `strategy` git applies
/// the user's configuration; when it has none and the branches diverged the error is
/// `DivergedBranches` and the interface asks which strategy to use.
pub fn pull(
    ctx: &OpCtx,
    io: &LongIo,
    strategy: Option<PullStrategy>,
) -> Result<RemoteResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let mut args = vec!["pull", "--progress"];
    match strategy {
        Some(PullStrategy::FastForwardOnly) => args.push("--ff-only"),
        Some(PullStrategy::Merge) => args.push("--no-rebase"),
        Some(PullStrategy::Rebase) => args.push("--rebase"),
        None => {}
    }
    let output = ctx
        .runner()
        .run_long_checked(ctx.workdir, &args, RunOptions::default(), io)?;
    finish(ctx, output)
}

/// What to push and how.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushOptions {
    /// The remote to push to; the upstream of the current branch when `None`.
    pub remote: Option<String>,
    /// The local branch to push; the current one when `None`.
    pub branch: Option<String>,
    /// Make the remote branch the upstream of the local one.
    pub set_upstream: bool,
    /// Overwrite the remote branch even if it has commits we do not, unless someone pushed
    /// since we last fetched (`--force-with-lease`).
    pub force_with_lease: bool,
}

pub fn push(ctx: &OpCtx, io: &LongIo, options: &PushOptions) -> Result<RemoteResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let mut args = vec!["push", "--progress"];
    if options.force_with_lease {
        args.push("--force-with-lease");
    }
    if options.set_upstream {
        args.push("-u");
    }
    let branch;
    if let Some(remote) = options.remote.as_deref() {
        check_name("remote", remote)?;
        args.push(remote);
        branch = match options.branch.clone() {
            Some(branch) => branch,
            None => ctx
                .status()?
                .branch
                .ok_or_else(|| GitError::other("HEAD is detached: choose a branch to push."))?,
        };
        check_name("branch", &branch)?;
        args.push(&branch);
    } else if let Some(name) = options.branch.as_deref() {
        // A branch without a remote: it goes to its own upstream.
        check_name("branch", name)?;
    }
    let output = ctx
        .runner()
        .run_long_checked(ctx.workdir, &args, RunOptions::default(), io)?;
    finish(ctx, output)
}

/// Deletes a branch on the remote (`git push <remote> --delete <branch>`).
pub fn delete_remote_branch(
    ctx: &OpCtx,
    io: &LongIo,
    remote: &str,
    branch: &str,
) -> Result<RemoteResult, GitError> {
    check_name("remote", remote)?;
    check_name("branch", branch)?;
    let _write = ctx.stores.begin_write()?;
    let output = ctx.runner().run_long_checked(
        ctx.workdir,
        &["push", "--progress", remote, "--delete", branch],
        RunOptions::default(),
        io,
    )?;
    finish(ctx, output)
}
