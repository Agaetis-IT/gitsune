//! Keeping `git status` fast on large working trees.
//!
//! On a repository of 100,000 files most of the time of `git status` goes to listing the
//! folders to find untracked files. git can remember that listing (the *untracked cache*), but
//! it has to be switched on in the repository's configuration. That is a change to the user's
//! repository, so it is never done silently: the interface offers it when `git status` is slow,
//! and the commands are listed in the activity log.

use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Serialize;

use super::cli::{GitError, RunOptions};
use super::ops::OpCtx;

/// A status slower than this is worth offering to speed up.
pub const SLOW_STATUS_MS: u64 = 400;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusHint {
    /// How long the last `git status` took.
    pub last_ms: u64,
    /// The untracked cache is already on.
    pub untracked_cache: bool,
    /// Slow, and the untracked cache would help: the interface may offer to switch it on.
    pub can_speed_up: bool,
}

fn untracked_cache_on(ctx: &OpCtx) -> Result<bool, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["config", "--get", "--type=bool", "core.untrackedCache"],
        RunOptions::background(),
    )?;
    Ok(output.code == 0 && output.stdout_text().trim() == "true")
}

pub fn status_hint(ctx: &OpCtx) -> Result<StatusHint, GitError> {
    let last_ms = ctx.stores.last_status_ms.load(Ordering::SeqCst);
    let untracked_cache = untracked_cache_on(ctx)?;
    Ok(StatusHint {
        last_ms,
        untracked_cache,
        can_speed_up: last_ms >= SLOW_STATUS_MS && !untracked_cache,
    })
}

/// Switches on the untracked cache of this repository (`core.untrackedCache`), after checking
/// that the file system supports it, and fills it with one status. Returns how long a status
/// takes afterwards.
pub fn enable_untracked_cache(ctx: &OpCtx) -> Result<StatusHint, GitError> {
    let _write = ctx.stores.begin_write()?;
    let test = ctx.runner().run_raw(
        ctx.workdir,
        &["update-index", "--test-untracked-cache"],
        RunOptions::default(),
    )?;
    if test.code != 0 {
        return Err(GitError::other(
            "This file system cannot keep git's untracked cache, so it was left off.",
        ));
    }
    let runner = ctx.runner();
    runner.run(
        ctx.workdir,
        &["config", "--local", "core.untrackedCache", "true"],
        RunOptions::default(),
    )?;
    runner.run(
        ctx.workdir,
        &["update-index", "--untracked-cache"],
        RunOptions::default(),
    )?;
    // A status that may write the index fills the cache.
    let timer = Instant::now();
    runner.run(ctx.workdir, &["status", "--short"], RunOptions::default())?;
    let _ = timer;
    ctx.status()?;
    status_hint(ctx)
}
