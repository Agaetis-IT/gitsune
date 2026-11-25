//! Fetch, pull, push and deleting a remote branch.
//!
//! These commands can take a while and talk to the network, so they run through
//! [`Runner::run_long`]: progress is streamed, secrets are asked through the askpass helper,
//! and the user can cancel. Authentication is the system's own (credential helpers, ssh-agent).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::cli::{ErrorKind, GitError, LongIo, RunOptions, RunOutput};
use super::ops::OpCtx;
use super::status::WorkingStatus;
use super::store::RepoStores;

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

/// A remote with the address it is fetched from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteUrl {
    pub name: String,
    pub url: String,
    /// The host the address points to. An SSH alias (`git@work-github:…`) is replaced by the
    /// host its configuration names, so the hosting service can be found. `None` for a local path.
    pub host: Option<String>,
}

/// The remotes and their fetch addresses.
pub fn remote_urls(ctx: &OpCtx) -> Result<Vec<RemoteUrl>, GitError> {
    let output = ctx
        .runner()
        .run(ctx.workdir, &["remote", "-v"], RunOptions::background())?;
    let mut urls = parse_remote_urls(&output.stdout_text());
    for remote in &mut urls {
        if !is_web_address(&remote.url) {
            remote.host = remote.host.as_deref().map(resolve_ssh_host);
        }
    }
    Ok(urls)
}

/// Reads `git remote -v`: `name<TAB>url (fetch)` lines (the push lines are the same address
/// unless a `pushurl` is set, which a pull request does not care about).
pub fn parse_remote_urls(text: &str) -> Vec<RemoteUrl> {
    let mut urls: Vec<RemoteUrl> = Vec::new();
    for line in text.lines() {
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        let Some(url) = rest.strip_suffix(" (fetch)") else {
            continue;
        };
        if !urls.iter().any(|known| known.name == name) {
            let url = url.trim().to_string();
            urls.push(RemoteUrl {
                name: name.to_string(),
                host: host_of_url(&url),
                url,
            });
        }
    }
    urls
}

fn is_web_address(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The host an address points to: `https://host/…`, `ssh://user@host:port/…` or `user@host:path`.
/// `None` for a local path.
pub fn host_of_url(url: &str) -> Option<String> {
    let host = match url.split_once("://") {
        Some((scheme, rest)) if !scheme.contains(['/', ':', '@']) => {
            let authority = rest.split('/').next()?;
            let host_port = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            host_port.split(':').next()?
        }
        _ => {
            let (before, _) = url.split_once(':')?;
            if before.contains('/') {
                return None;
            }
            before.rsplit_once('@').map_or(before, |(_, host)| host)
        }
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// The host name `ssh` uses for an alias, read from the configuration without connecting.
/// The alias itself when it cannot be read.
pub fn resolve_ssh_host(alias: &str) -> String {
    if !crate::hosting::valid_host(alias) {
        return alias.to_string();
    }
    let output = std::process::Command::new("ssh")
        .args(["-G", alias])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(output) = output else {
        return alias.to_string();
    };
    if !output.status.success() {
        return alias.to_string();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("hostname "))
        .map(|host| host.trim().to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| alias.to_string())
}

/// A folder name that stays inside the folder it is created in.
fn valid_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

/// Clones `url` into the folder `name` of `parent` and returns the path of the new working tree.
/// The folder may exist when it is empty. When the clone fails or is cancelled the folder this
/// call created is removed, so no half-cloned repository is left behind.
pub fn clone_repository(
    io: &LongIo,
    url: &str,
    parent: &Path,
    name: &str,
) -> Result<PathBuf, GitError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(GitError::other("Enter the address of the repository."));
    }
    // Read as an option otherwise; `--` below only protects the arguments after it.
    if url.starts_with('-') || url.chars().any(char::is_control) {
        return Err(GitError::other("This is not a valid repository address."));
    }
    if !valid_folder_name(name) {
        return Err(GitError::other("This is not a valid folder name."));
    }
    if !parent.is_dir() {
        return Err(GitError::other("The folder to clone into does not exist."));
    }
    let destination = parent.join(name);
    let existed = destination.exists();
    if existed && !is_empty_folder(&destination) {
        return Err(GitError::other(format!(
            "{name} already exists in that folder and is not empty."
        )));
    }
    let stores = RepoStores::default();
    let result = stores.runner().run_long_checked(
        parent,
        &["clone", "--progress", "--", url, name],
        RunOptions::default(),
        io,
    );
    match result {
        Ok(_) => Ok(destination),
        Err(error) => {
            if !existed {
                let _ = std::fs::remove_dir_all(&destination);
            }
            Err(error)
        }
    }
}

fn is_empty_folder(path: &Path) -> bool {
    path.is_dir()
        && std::fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
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
