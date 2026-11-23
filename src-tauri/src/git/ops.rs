//! Writes to a repository (and reads of its working tree) through the `git` binary.
//!
//! Every write takes the repository's write lock, is refused in read-only mode, is logged in
//! the activity log, and returns the status of the working tree afterwards so the interface
//! can update at once.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::cli::{ErrorKind, GitError, RunOptions, Runner};
use super::commit_detail::{
    ChangeStatus, FileDiff, MAX_AUTO_BLOB_BYTES, MAX_DIFF_LINES, MAX_FORCED_BLOB_BYTES,
};
use super::status::{WorkingStatus, read_status};
use super::store::RepoStores;
use super::text_diff::{hunk_patch, is_binary, text_diff};

/// What the operations need to know about an open repository.
pub struct OpCtx<'a> {
    /// Root of the working tree.
    pub workdir: &'a Path,
    pub stores: &'a RepoStores,
}

impl OpCtx<'_> {
    pub(super) fn runner(&self) -> Runner<'_> {
        self.stores.runner()
    }

    /// Current status of the working tree (a background read: not logged, no locks).
    pub fn status(&self) -> Result<WorkingStatus, GitError> {
        read_status(&self.runner(), self.workdir)
    }

    fn has_head(&self) -> Result<bool, GitError> {
        let output = self.runner().run_raw(
            self.workdir,
            &["rev-parse", "--verify", "-q", "HEAD"],
            RunOptions::background(),
        )?;
        Ok(output.code == 0)
    }

    /// Content of a blob at `spec` (`HEAD:path`, `:0:path`), `None` if it does not exist.
    pub(super) fn blob(&self, spec: &str) -> Result<Option<Vec<u8>>, GitError> {
        let output = self.runner().run_raw(
            self.workdir,
            &["cat-file", "blob", spec],
            RunOptions::background(),
        )?;
        Ok((output.code == 0).then_some(output.stdout))
    }

    fn blob_size(&self, spec: &str) -> Result<Option<u64>, GitError> {
        let output = self.runner().run_raw(
            self.workdir,
            &["cat-file", "-s", spec],
            RunOptions::background(),
        )?;
        Ok((output.code == 0)
            .then(|| output.stdout_text().trim().parse().ok())
            .flatten())
    }
}

fn check_paths(paths: &[String]) -> Result<(), GitError> {
    if paths.iter().any(|p| p.is_empty() || p.contains('\0')) {
        return Err(GitError::other("invalid path"));
    }
    Ok(())
}

/// Runs a command that takes many paths in chunks, so the command line stays short.
fn run_with_paths(ctx: &OpCtx, args: &[&str], paths: &[String]) -> Result<(), GitError> {
    for chunk in paths.chunks(400) {
        let mut full: Vec<&str> = args.to_vec();
        full.push("--");
        full.extend(chunk.iter().map(String::as_str));
        ctx.runner()
            .run(ctx.workdir, &full, RunOptions::default())?;
    }
    Ok(())
}

/// Adds the changes of `paths` (new, modified or deleted files) to the index.
pub fn stage_paths(ctx: &OpCtx, paths: &[String]) -> Result<WorkingStatus, GitError> {
    check_paths(paths)?;
    let _write = ctx.stores.begin_write()?;
    if !paths.is_empty() {
        run_with_paths(ctx, &["add", "-A"], paths)?;
    }
    ctx.status()
}

/// Takes the changes of `paths` out of the index (the files keep their content).
pub fn unstage_paths(ctx: &OpCtx, paths: &[String]) -> Result<WorkingStatus, GitError> {
    check_paths(paths)?;
    let _write = ctx.stores.begin_write()?;
    if !paths.is_empty() {
        if ctx.has_head()? {
            run_with_paths(ctx, &["restore", "--staged"], paths)?;
        } else {
            // No commit yet: there is nothing to restore from, just forget the files.
            run_with_paths(ctx, &["rm", "--cached", "-r", "-q"], paths)?;
        }
    }
    ctx.status()
}

fn worktree_file(ctx: &OpCtx, path: &str) -> Result<Option<Vec<u8>>, GitError> {
    let full = ctx.workdir.join(path);
    match fs::symlink_metadata(&full) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(fs::read_link(&full)
            .ok()
            .map(|target| target.to_string_lossy().into_owned().into_bytes())),
        Ok(meta) if meta.is_file() => fs::read(&full)
            .map(Some)
            .map_err(|e| GitError::other(format!("cannot read {path}: {e}"))),
        _ => Ok(None),
    }
}

fn apply_patch(ctx: &OpCtx, patch: Vec<u8>, reverse: bool) -> Result<(), GitError> {
    let mut args = vec!["apply", "--cached", "--recount"];
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    ctx.runner()
        .run(ctx.workdir, &args, RunOptions::with_stdin(patch))?;
    Ok(())
}

/// Adds one hunk of the unstaged changes of a file to the index.
pub fn stage_hunk(ctx: &OpCtx, path: &str, hunk_id: &str) -> Result<WorkingStatus, GitError> {
    check_paths(&[path.to_string()])?;
    let _write = ctx.stores.begin_write()?;
    let old = ctx.blob(&format!(":0:{path}"))?.ok_or_else(|| {
        GitError::other("This file is not tracked yet: stage the whole file instead.")
    })?;
    let new = worktree_file(ctx, path)?
        .ok_or_else(|| GitError::other("The file no longer exists: stage its deletion instead."))?;
    if is_binary(&old) || is_binary(&new) {
        return Err(GitError::other(
            "Binary files can only be staged as a whole.",
        ));
    }
    let patch = hunk_patch(path, &old, &new, hunk_id).ok_or_else(stale)?;
    apply_patch(ctx, patch, false)?;
    ctx.status()
}

/// Takes one hunk of the staged changes of a file out of the index.
pub fn unstage_hunk(ctx: &OpCtx, path: &str, hunk_id: &str) -> Result<WorkingStatus, GitError> {
    check_paths(&[path.to_string()])?;
    let _write = ctx.stores.begin_write()?;
    let staged = ctx
        .blob(&format!(":0:{path}"))?
        .ok_or_else(|| GitError::other("The file is not in the index."))?;
    let committed = ctx.blob(&format!("HEAD:{path}"))?.unwrap_or_default();
    if is_binary(&committed) || is_binary(&staged) {
        return Err(GitError::other(
            "Binary files can only be unstaged as a whole.",
        ));
    }
    let patch = hunk_patch(path, &committed, &staged, hunk_id).ok_or_else(stale)?;
    apply_patch(ctx, patch, true)?;
    ctx.status()
}

fn stale() -> GitError {
    GitError::new(
        ErrorKind::StaleHunk,
        "The file changed since this hunk was shown. The diff has been reloaded.",
    )
}

/// Which side of the index a working tree diff is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Area {
    /// The index against the last commit (what the next commit will contain).
    Staged,
    /// The working tree against the index (untracked files count as entirely new).
    Unstaged,
}

/// Diff of one file of the working tree, for the staged or the unstaged side.
///
/// `old_path` is the previous path of a renamed file (staged side only). Files over 1 MiB
/// are only diffed with `force`, as for commits.
pub fn working_diff(
    ctx: &OpCtx,
    path: &str,
    old_path: Option<&str>,
    area: Area,
    force: bool,
) -> Result<FileDiff, GitError> {
    check_paths(&[path.to_string()])?;
    let old_at = old_path.unwrap_or(path);
    let (old_spec, new_spec) = match area {
        Area::Staged => (format!("HEAD:{old_at}"), Some(format!(":0:{path}"))),
        Area::Unstaged => (format!(":0:{path}"), None),
    };

    let old_size = ctx.blob_size(&old_spec)?;
    let new_size = match &new_spec {
        Some(spec) => ctx.blob_size(spec)?,
        None => fs::symlink_metadata(ctx.workdir.join(path))
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len()),
    };

    let status = match (old_size.is_some(), new_size.is_some()) {
        (false, true) => ChangeStatus::Added,
        (true, false) => ChangeStatus::Deleted,
        _ if old_at != path => ChangeStatus::Renamed,
        _ => ChangeStatus::Modified,
    };
    let mut result = FileDiff {
        path: path.to_string(),
        old_path: (old_at != path).then(|| old_at.to_string()),
        status,
        binary: false,
        submodule: false,
        too_large: false,
        can_force: false,
        truncated: false,
        old_size,
        new_size,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
    };

    let biggest = old_size.unwrap_or(0).max(new_size.unwrap_or(0));
    let limit = if force {
        MAX_FORCED_BLOB_BYTES
    } else {
        MAX_AUTO_BLOB_BYTES
    };
    if biggest > limit {
        result.too_large = true;
        result.can_force = biggest <= MAX_FORCED_BLOB_BYTES;
        return Ok(result);
    }

    let old = ctx.blob(&old_spec)?.unwrap_or_default();
    let new = match &new_spec {
        Some(spec) => ctx.blob(spec)?.unwrap_or_default(),
        None => worktree_file(ctx, path)?.unwrap_or_default(),
    };
    if is_binary(&old) || is_binary(&new) {
        result.binary = true;
        return Ok(result);
    }
    let diff = text_diff(&old, &new, MAX_DIFF_LINES);
    result.additions = diff.additions;
    result.deletions = diff.deletions;
    result.truncated = diff.truncated;
    result.hunks = diff.hunks;
    Ok(result)
}

// ---------- commits ----------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    /// The new commit.
    pub id: String,
    /// What HEAD was before (`None` for the first commit), to undo the commit.
    pub before: Option<String>,
    pub status: WorkingStatus,
}

fn head_id(ctx: &OpCtx) -> Result<Option<String>, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["rev-parse", "--verify", "-q", "HEAD"],
        RunOptions::background(),
    )?;
    Ok((output.code == 0).then(|| output.stdout_text().trim().to_string()))
}

/// Whether the repository has a hook that can reject a commit.
fn has_commit_hooks(ctx: &OpCtx) -> bool {
    let Ok(output) = ctx.runner().run_raw(
        ctx.workdir,
        &["rev-parse", "--git-path", "hooks"],
        RunOptions::background(),
    ) else {
        return false;
    };
    let dir = ctx.workdir.join(output.stdout_text().trim());
    ["pre-commit", "prepare-commit-msg", "commit-msg"]
        .iter()
        .any(|name| dir.join(name).is_file())
}

/// Commits the index (or, with `amend`, replaces the last commit).
///
/// Hooks and signing behave as in the terminal: this runs `git commit`. A rejected commit
/// comes back as [`ErrorKind::HookFailed`] with the hook's output, a missing identity as
/// [`ErrorKind::NeedsIdentity`], a failed signature as [`ErrorKind::SigningFailed`].
pub fn commit(
    ctx: &OpCtx,
    summary: &str,
    body: &str,
    amend: bool,
) -> Result<CommitResult, GitError> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(GitError::other("The commit message needs a summary."));
    }
    let _write = ctx.stores.begin_write()?;
    let before = head_id(ctx)?;
    if amend && before.is_none() {
        return Err(GitError::other("There is no commit to amend yet."));
    }

    let mut message = summary.to_string();
    if !body.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(body.trim_end());
    }
    message.push('\n');

    let mut args = vec!["commit", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &args,
        RunOptions::with_stdin(message.into_bytes()),
    )?;
    if output.code != 0 {
        let kind =
            super::cli::classify(&output.stdout_text(), &output.stderr).unwrap_or_else(|| {
                if has_commit_hooks(ctx) {
                    ErrorKind::HookFailed
                } else {
                    ErrorKind::Other
                }
            });
        return Err(GitError::new(kind, output.failure_text()));
    }

    let id = head_id(ctx)?
        .ok_or_else(|| GitError::other("the commit was made but HEAD cannot be read"))?;
    Ok(CommitResult {
        id,
        before,
        status: ctx.status()?,
    })
}

/// Moves HEAD back to where it was before `commit`, keeping the changes staged.
///
/// Refused unless HEAD is still the commit that was made (nothing else happened since).
pub fn undo_commit(
    ctx: &OpCtx,
    before: Option<&str>,
    after: &str,
) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    if head_id(ctx)?.as_deref() != Some(after) {
        return Err(GitError::other(
            "HEAD has moved since that commit: it cannot be undone from here.",
        ));
    }
    match before {
        Some(before) => {
            ctx.runner().run(
                ctx.workdir,
                &["reset", "--soft", before],
                RunOptions::default(),
            )?;
        }
        // The very first commit: put the branch back to "no commits yet".
        None => {
            ctx.runner().run(
                ctx.workdir,
                &["update-ref", "-d", "HEAD"],
                RunOptions::default(),
            )?;
        }
    }
    ctx.status()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
}

fn config_value(ctx: &OpCtx, key: &str) -> Result<Option<String>, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["config", "--get", key],
        RunOptions::background(),
    )?;
    let value = output.stdout_text().trim().to_string();
    Ok((output.code == 0 && !value.is_empty()).then_some(value))
}

/// The name and email commits will be made with.
pub fn identity(ctx: &OpCtx) -> Result<Identity, GitError> {
    Ok(Identity {
        name: config_value(ctx, "user.name")?,
        email: config_value(ctx, "user.email")?,
    })
}

/// Sets `user.name` and `user.email`, for every repository (`global`) or only this one.
pub fn set_identity(
    ctx: &OpCtx,
    name: &str,
    email: &str,
    global: bool,
) -> Result<Identity, GitError> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || email.is_empty() || name.contains('\n') || email.contains('\n') {
        return Err(GitError::other("A name and an email address are needed."));
    }
    let _write = ctx.stores.begin_write()?;
    let scope = if global { "--global" } else { "--local" };
    for (key, value) in [("user.name", name), ("user.email", email)] {
        ctx.runner().run(
            ctx.workdir,
            &["config", scope, key, value],
            RunOptions::default(),
        )?;
    }
    identity(ctx)
}

// ---------- discarding and recovering changes ----------

/// Where snapshots of discarded changes are kept (as commits no branch points at).
pub const SNAPSHOT_PREFIX: &str = "refs/git-hydra/undo/";
/// Snapshots kept; older ones are removed when a new one is made.
pub const MAX_SNAPSHOTS: usize = 20;

/// The working tree content of some files, saved before they were discarded or overwritten.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Full name of the reference holding it.
    pub reference: String,
    pub id: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub description: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardResult {
    /// Made before anything was removed; restoring it brings the changes back.
    pub snapshot: Option<Snapshot>,
    pub status: WorkingStatus,
}

static SNAPSHOT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const SNAPSHOT_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "git-hydra"),
    ("GIT_AUTHOR_EMAIL", "git-hydra@localhost"),
    ("GIT_COMMITTER_NAME", "git-hydra"),
    ("GIT_COMMITTER_EMAIL", "git-hydra@localhost"),
];

fn env_of(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn is_tracked(ctx: &OpCtx, path: &str) -> Result<bool, GitError> {
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["ls-files", "--error-unmatch", "--", path],
        RunOptions::background(),
    )?;
    Ok(output.code == 0)
}

/// Saves the current content of `paths` (including untracked files and deletions) as a
/// commit under [`SNAPSHOT_PREFIX`], using a throwaway index so the real one is not touched.
fn take_snapshot(ctx: &OpCtx, paths: &[String], what: &str) -> Result<Snapshot, GitError> {
    let git_dir = ctx
        .runner()
        .run(
            ctx.workdir,
            &["rev-parse", "--absolute-git-dir"],
            RunOptions::background(),
        )?
        .stdout_text()
        .trim()
        .to_string();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let index_file =
        Path::new(&git_dir).join(format!("git-hydra-snapshot-{stamp}-{}", std::process::id()));
    let index_env = || {
        let mut env = env_of(&SNAPSHOT_IDENTITY);
        env.push((
            "GIT_INDEX_FILE".into(),
            index_file.to_string_lossy().into_owned(),
        ));
        env
    };

    let result = (|| -> Result<Snapshot, GitError> {
        let has_head = ctx.has_head()?;
        if has_head {
            ctx.runner().run(
                ctx.workdir,
                &["read-tree", "HEAD"],
                RunOptions::with_env(index_env()),
            )?;
        }
        // `git add` refuses a path that exists nowhere (an untracked file already gone).
        let mut present = Vec::new();
        for path in paths {
            if fs::symlink_metadata(ctx.workdir.join(path)).is_ok() || is_tracked(ctx, path)? {
                present.push(path.clone());
            }
        }
        for chunk in present.chunks(400) {
            let mut args = vec!["add", "-A", "--"];
            args.extend(chunk.iter().map(String::as_str));
            ctx.runner()
                .run(ctx.workdir, &args, RunOptions::with_env(index_env()))?;
        }
        let tree = ctx
            .runner()
            .run(
                ctx.workdir,
                &["write-tree"],
                RunOptions::with_env(index_env()),
            )?
            .stdout_text()
            .trim()
            .to_string();

        let mut message = format!("git-hydra: before {what} ({} file(s))\n", paths.len());
        message.push('\n');
        for path in paths {
            message.push_str(path);
            message.push('\n');
        }
        let mut args = vec!["commit-tree", tree.as_str(), "-F", "-"];
        let head = head_id(ctx)?;
        if let Some(head) = head.as_deref() {
            args.extend(["-p", head]);
        }
        let mut options = RunOptions::with_env(env_of(&SNAPSHOT_IDENTITY));
        options.stdin = Some(message.into_bytes());
        let id = ctx
            .runner()
            .run(ctx.workdir, &args, options)?
            .stdout_text()
            .trim()
            .to_string();

        // Millisecond time then a counter: names sort in creation order and never collide.
        let seq = SNAPSHOT_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let reference = format!("{SNAPSHOT_PREFIX}{stamp:013}-{seq:06}");
        ctx.runner().run(
            ctx.workdir,
            &["update-ref", &reference, &id],
            RunOptions::default(),
        )?;
        Ok(Snapshot {
            reference,
            id,
            time: (stamp / 1000) as i64,
            description: format!("before {what}"),
            paths: paths.to_vec(),
        })
    })();
    let _ = fs::remove_file(&index_file);

    let snapshot = result?;
    purge_snapshots(ctx)?;
    Ok(snapshot)
}

/// Removes the oldest snapshots beyond [`MAX_SNAPSHOTS`].
fn purge_snapshots(ctx: &OpCtx) -> Result<(), GitError> {
    let snapshots = list_snapshots_inner(ctx)?;
    for old in snapshots.iter().skip(MAX_SNAPSHOTS) {
        ctx.runner().run(
            ctx.workdir,
            &["update-ref", "-d", &old.reference],
            RunOptions::default(),
        )?;
    }
    Ok(())
}

fn list_snapshots_inner(ctx: &OpCtx) -> Result<Vec<Snapshot>, GitError> {
    let output = ctx.runner().run(
        ctx.workdir,
        &[
            "for-each-ref",
            "--sort=-refname",
            "--format=%(refname)%1f%(objectname)%1f%(creatordate:unix)%1f%(contents)%1e",
            SNAPSHOT_PREFIX,
        ],
        RunOptions::background(),
    )?;
    let text = output.stdout_text();
    let mut snapshots = Vec::new();
    for record in text
        .split('\u{1e}')
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        let mut parts = record.splitn(4, '\u{1f}');
        let (Some(reference), Some(id), Some(time), Some(contents)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let mut lines = contents.lines();
        let description = lines
            .next()
            .unwrap_or_default()
            .trim_start_matches("git-hydra: ")
            .to_string();
        let paths = lines
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        snapshots.push(Snapshot {
            reference: reference.to_string(),
            id: id.to_string(),
            time: time.parse().unwrap_or(0),
            description,
            paths,
        });
    }
    Ok(snapshots)
}

/// The saved snapshots, newest first.
pub fn list_snapshots(ctx: &OpCtx) -> Result<Vec<Snapshot>, GitError> {
    list_snapshots_inner(ctx)
}

/// Throws away the unstaged changes of `paths`: modified and deleted files go back to their
/// staged content, untracked files are deleted. Staged changes are kept. A snapshot is taken
/// first, so [`restore_snapshot`] can bring everything back.
pub fn discard_paths(ctx: &OpCtx, paths: &[String]) -> Result<DiscardResult, GitError> {
    check_paths(paths)?;
    let _write = ctx.stores.begin_write()?;
    let status = ctx.status()?;
    let wanted: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for entry in status
        .unstaged
        .iter()
        .filter(|e| wanted.contains(e.path.as_str()))
    {
        if entry.state == super::status::FileState::Untracked {
            untracked.push(entry.path.clone());
        } else {
            tracked.push(entry.path.clone());
        }
    }
    if tracked.is_empty() && untracked.is_empty() {
        return Ok(DiscardResult {
            snapshot: None,
            status,
        });
    }

    let all: Vec<String> = tracked.iter().chain(&untracked).cloned().collect();
    let snapshot = take_snapshot(ctx, &all, "discarding changes")?;
    if !tracked.is_empty() {
        run_with_paths(ctx, &["restore"], &tracked)?;
    }
    if !untracked.is_empty() {
        run_with_paths(ctx, &["clean", "-f", "-d"], &untracked)?;
    }
    Ok(DiscardResult {
        snapshot: Some(snapshot),
        status: ctx.status()?,
    })
}

/// Puts the files of a snapshot back in the working tree (files that did not exist in it are
/// removed). What is overwritten is itself saved first, so this can be undone too.
pub fn restore_snapshot(ctx: &OpCtx, reference: &str) -> Result<DiscardResult, GitError> {
    if !reference.starts_with(SNAPSHOT_PREFIX) || reference.contains("..") {
        return Err(GitError::other("not a snapshot of this application"));
    }
    let _write = ctx.stores.begin_write()?;
    let snapshot = list_snapshots_inner(ctx)?
        .into_iter()
        .find(|s| s.reference == reference)
        .ok_or_else(|| GitError::other("That snapshot no longer exists."))?;

    let current = take_snapshot(ctx, &snapshot.paths, "restoring discarded changes")?;
    for path in &snapshot.paths {
        let in_snapshot = ctx.runner().run_raw(
            ctx.workdir,
            &["cat-file", "-e", &format!("{}:{path}", snapshot.id)],
            RunOptions::background(),
        )?;
        if in_snapshot.code == 0 {
            let source = format!("--source={}", snapshot.id);
            ctx.runner().run(
                ctx.workdir,
                &["restore", &source, "--worktree", "--", path],
                RunOptions::default(),
            )?;
        } else {
            let full = ctx.workdir.join(path);
            if full.is_dir() {
                let _ = fs::remove_dir_all(&full);
            } else {
                let _ = fs::remove_file(&full);
            }
        }
    }
    ctx.runner().run(
        ctx.workdir,
        &["update-ref", "-d", &snapshot.reference],
        RunOptions::default(),
    )?;
    Ok(DiscardResult {
        snapshot: Some(current),
        status: ctx.status()?,
    })
}
