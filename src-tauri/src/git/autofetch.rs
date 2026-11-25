//! The fetch the application makes by itself, in the background. It must never ask the user
//! anything: no askpass helper, no terminal prompt, ssh in batch mode. What the keychain and
//! ssh-agent know is used; anything else makes it fail quietly and the user fetches by hand.

use serde::Serialize;

use super::cli::{GitError, RunOptions};
use super::ops::OpCtx;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BranchUpdate {
    pub branch: String,
    pub upstream: String,
    /// Commits the fetch brought for this branch.
    pub new_commits: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutoFetchResult {
    /// Commits waiting to be pulled, over every branch that has an upstream.
    pub behind_total: usize,
    /// The branches that moved because of this fetch.
    pub updates: Vec<BranchUpdate>,
}

/// `ahead 1, behind 3` gives 3.
fn behind_of(track: &str) -> usize {
    track
        .split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("behind "))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// For each local branch with an upstream: its name, the upstream and the commits it is behind.
fn behind_counts(ctx: &OpCtx) -> Result<Vec<(String, String, usize)>, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &[
            "for-each-ref",
            "--format=%(refname:short)%1f%(upstream:short)%1f%(upstream:track,nobracket)",
            "refs/heads",
        ],
        RunOptions::background(),
    )?;
    if output.code != 0 {
        return Ok(Vec::new());
    }
    Ok(output
        .stdout_text()
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\u{1f}');
            let branch = parts.next()?.to_string();
            let upstream = parts.next()?.to_string();
            let track = parts.next().unwrap_or("");
            (!upstream.is_empty()).then(|| (branch, upstream, behind_of(track)))
        })
        .collect())
}

/// ssh told never to ask: the user's own ssh command (or `ssh`) with batch mode and a timeout.
fn quiet_ssh_command(ctx: &OpCtx) -> String {
    let own = std::env::var("GIT_SSH_COMMAND")
        .ok()
        .filter(|c| !c.trim().is_empty());
    let configured = || {
        ctx.runner()
            .run_raw(
                ctx.workdir,
                &["config", "--get", "core.sshCommand"],
                RunOptions::background(),
            )
            .ok()
            .filter(|out| out.code == 0)
            .map(|out| out.stdout_text().trim().to_string())
            .filter(|c| !c.is_empty())
    };
    let base = own.or_else(configured).unwrap_or_else(|| "ssh".to_string());
    format!("{base} -o BatchMode=yes -o ConnectTimeout=15")
}

/// Fetches every remote without ever asking a question, and tells which tracked branches
/// received commits. A repository without remotes is left alone.
pub fn auto_fetch(ctx: &OpCtx) -> Result<AutoFetchResult, GitError> {
    let remotes = ctx
        .runner()
        .run_raw(ctx.workdir, &["remote"], RunOptions::background())?;
    if remotes.code != 0 || remotes.stdout_text().trim().is_empty() {
        return Ok(AutoFetchResult::default());
    }
    let before = behind_counts(ctx)?;
    let env = vec![
        ("GIT_SSH_COMMAND".to_string(), quiet_ssh_command(ctx)),
        // A web remote that stops answering must not hold the app for ever.
        ("GIT_HTTP_LOW_SPEED_LIMIT".to_string(), "1000".to_string()),
        ("GIT_HTTP_LOW_SPEED_TIME".to_string(), "30".to_string()),
        // No helper to ask a secret: a missing one is a failure.
        ("GIT_ASKPASS".to_string(), "true".to_string()),
        ("SSH_ASKPASS".to_string(), "true".to_string()),
    ];
    ctx.runner().run(
        ctx.workdir,
        &["fetch", "--all", "--quiet"],
        RunOptions::with_env(env),
    )?;
    let after = behind_counts(ctx)?;
    let updates = after
        .iter()
        .filter_map(|(branch, upstream, behind)| {
            let was = before
                .iter()
                .find(|(b, _, _)| b == branch)
                .map_or(0, |(_, _, n)| *n);
            (*behind > was).then(|| BranchUpdate {
                branch: branch.clone(),
                upstream: upstream.clone(),
                new_commits: behind - was,
            })
        })
        .collect();
    Ok(AutoFetchResult {
        behind_total: after.iter().map(|(_, _, n)| n).sum(),
        updates,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn the_behind_count_is_read_from_the_track_text() {
        assert_eq!(behind_of("ahead 1, behind 3"), 3);
        assert_eq!(behind_of("behind 12"), 12);
        assert_eq!(behind_of("ahead 2"), 0);
        assert_eq!(behind_of("gone"), 0);
        assert_eq!(behind_of(""), 0);
    }
}
