//! Rebase: replay the commits of the current branch on top of another one, optionally after
//! editing the list of commits (reorder, reword, edit, squash, fixup, drop).
//!
//! Plain rebases are `git rebase <onto>`. For the interactive one git is started with
//! `GIT_SEQUENCE_EDITOR` pointing to a tiny script that puts the list built in the app in
//! place of the one git proposes, so what runs is exactly what was shown. New messages
//! (reword, squash) are given to git through `GIT_EDITOR`: a second script that git starts when
//! it asks for a message, which recognises the commit from the command git is running and
//! writes the message chosen in the app. It is git itself that makes the commit, so a failure (a hook, a signing
//! tool waiting for approval) stops the rebase in the ordinary way and *Continue* tries again.
//! The scripts and messages live in the git directory: a rebase that stops on a conflict and is
//! continued later still finds them (see [`rebase_env`]).

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::branches::{fallback_identity, resolve};
use super::cli::{GitError, RunOptions, error_from};
use super::ops::{OpCtx, head_id};
use super::status::{OperationKind, WorkingStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseCommit {
    pub id: String,
    pub summary: String,
}

/// What rebasing onto a target would replay, to show it before asking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlan {
    /// The commits of the current branch that are not in the target, oldest first. Merge commits
    /// are not listed: a plain rebase leaves them out and replays what they brought.
    pub commits: Vec<RebaseCommit>,
    pub merges: u32,
    /// How many of those commits are already on the upstream: rewriting them means the branch
    /// will have to be force-pushed.
    pub pushed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TodoAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl TodoAction {
    fn word(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reword => "reword",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub action: TodoAction,
    pub id: String,
    /// The message of the commit once done (for `reword`, and for a `squash` whose combined
    /// message was edited); `None` keeps git's own.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RebaseOutcome {
    UpToDate,
    Done,
    /// Stopped for the user to edit a commit (`edit`): amend, then continue.
    Stopped,
    /// Stopped on conflicts: resolve them, then continue (or skip, or abort).
    Conflicts,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseResult {
    pub status: WorkingStatus,
    pub outcome: RebaseOutcome,
    /// The branch tip before and after: what Undo needs.
    pub before: String,
    pub after: String,
}

fn check_target(onto: &str) -> Result<(), GitError> {
    if onto.trim().is_empty() || onto.starts_with('-') || onto.contains('\0') {
        return Err(GitError::other(format!("Cannot rebase onto '{onto}'.")));
    }
    Ok(())
}

fn count(ctx: &OpCtx, args: &[&str]) -> Result<u32, GitError> {
    let mut full = vec!["rev-list", "--count"];
    full.extend(args);
    let output = ctx
        .runner()
        .run(ctx.workdir, &full, RunOptions::background())?;
    Ok(output.stdout_text().trim().parse().unwrap_or(0))
}

pub fn rebase_plan(ctx: &OpCtx, onto: &str) -> Result<RebasePlan, GitError> {
    check_target(onto)?;
    let target = resolve(ctx, onto)?;
    let range = format!("{target}..HEAD");
    let log = ctx.runner().run(
        ctx.workdir,
        &[
            "log",
            "--reverse",
            "--topo-order",
            "--no-merges",
            "--format=%H%x1f%s%x1e",
            &range,
        ],
        RunOptions::background(),
    )?;
    let commits: Vec<RebaseCommit> = log
        .stdout_text()
        .split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let (id, summary) = record.split_once('\x1f')?;
            Some(RebaseCommit {
                id: id.to_string(),
                summary: summary.to_string(),
            })
        })
        .collect();
    let merges = count(ctx, &["--merges", &range])?;
    // The upstream may be gone (its branch was deleted on the server after a merged pull
    // request): then nothing can be said to be on it, and that must not stop the rebase.
    let pushed = match ctx.status()?.upstream {
        Some(upstream) if !commits.is_empty() && resolve(ctx, &upstream).is_ok() => {
            let total = count(ctx, &["--no-merges", &range])?;
            let unpushed = count(ctx, &["--no-merges", &range, "--not", &upstream])?;
            total.saturating_sub(unpushed)
        }
        _ => 0,
    };
    Ok(RebasePlan {
        commits,
        merges,
        pushed,
    })
}

/// The directory in the git directory where the files of an interactive rebase live.
fn helper_dir(ctx: &OpCtx) -> Result<PathBuf, GitError> {
    let output = ctx.runner().run(
        ctx.workdir,
        &["rev-parse", "--absolute-git-dir"],
        RunOptions::background(),
    )?;
    Ok(PathBuf::from(output.stdout_text().trim()).join("git-hydra-rebase"))
}

fn shell_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Validates the list against the commits git would replay and writes the files git needs.
/// Returns the environment of the run.
fn prepare_todo(
    ctx: &OpCtx,
    plan: &RebasePlan,
    items: &[TodoItem],
) -> Result<Vec<(String, String)>, GitError> {
    let planned: HashSet<&str> = plan.commits.iter().map(|c| c.id.as_str()).collect();
    let given: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    if items.len() != plan.commits.len() || planned != given {
        return Err(GitError::other(
            "The commits changed since the list was made: open the rebase again.",
        ));
    }
    let first = items.iter().find(|i| i.action != TodoAction::Drop);
    if let Some(first) = first
        && matches!(first.action, TodoAction::Squash | TodoAction::Fixup)
    {
        return Err(GitError::other(
            "The first commit cannot be squashed or fixed up: there is nothing before it.",
        ));
    }

    let dir = helper_dir(ctx)?;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)
        .map_err(|e| GitError::other(format!("cannot prepare the rebase: {e}")))?;
    let summary = |id: &str| {
        plan.commits
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.summary.replace(['\n', '\r'], " "))
            .unwrap_or_default()
    };

    let mut todo = String::new();
    for item in items {
        todo.push_str(&format!(
            "{} {} {}\n",
            item.action.word(),
            item.id,
            summary(&item.id)
        ));
    }

    // The messages to put in: one per commit that ends up with an edited message. git asks for
    // a message while it runs the command of the last commit of a group (a commit with
    // squashed ones after it), so every commit of the group points at the same message.
    let clean = |item: &TodoItem| {
        item.message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
    };
    let mut messages = String::new();
    let mut groups: Vec<(Vec<&str>, Option<String>)> = Vec::new();
    for item in items {
        match item.action {
            TodoAction::Drop => {}
            TodoAction::Squash | TodoAction::Fixup => {
                if let Some((ids, message)) = groups.last_mut() {
                    ids.push(&item.id);
                    if item.action == TodoAction::Squash
                        && let Some(own) = clean(item)
                    {
                        *message = Some(own);
                    }
                }
            }
            TodoAction::Pick | TodoAction::Edit | TodoAction::Reword => {
                let message = (item.action == TodoAction::Reword)
                    .then(|| clean(item))
                    .flatten();
                groups.push((vec![&item.id], message));
            }
        }
    }
    for (index, (ids, message)) in groups.iter().enumerate() {
        let Some(message) = message else { continue };
        let file = dir.join(format!("message-{index}"));
        fs::write(&file, format!("{message}\n"))
            .map_err(|e| GitError::other(format!("cannot prepare the rebase: {e}")))?;
        for id in ids {
            messages.push_str(&format!("{id}\n{}\n", file.display()));
        }
    }

    let write_file = |name: &str, content: &str, executable: bool| -> Result<PathBuf, GitError> {
        let file = dir.join(name);
        fs::write(&file, content)
            .map_err(|e| GitError::other(format!("cannot prepare the rebase: {e}")))?;
        #[cfg(unix)]
        if executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700))
                .map_err(|e| GitError::other(format!("cannot prepare the rebase: {e}")))?;
        }
        let _ = executable;
        Ok(file)
    };
    let todo_file = write_file("todo", &todo, false)?;
    write_file("messages", &messages, false)?;
    let sequence_editor = write_file(
        "sequence-editor.sh",
        "#!/bin/sh\ncat \"$GIT_HYDRA_TODO\" > \"$1\"\n",
        true,
    )?;
    write_file("message-editor.sh", MESSAGE_EDITOR, true)?;
    let mut env = vec![
        (
            "GIT_SEQUENCE_EDITOR".to_string(),
            shell_quote(&sequence_editor),
        ),
        (
            "GIT_HYDRA_TODO".to_string(),
            todo_file.to_string_lossy().into_owned(),
        ),
    ];
    env.extend(rebase_env(ctx)?);
    Ok(env)
}

/// Writes the message chosen in the app into the file git asks the editor to fill. The commit is
/// told by the command git is running (the last line of `rebase-merge/done`), so a second try
/// after a failure (a hook, a signing tool waiting for approval) gets the same message.
const MESSAGE_EDITOR: &str = r#"#!/bin/sh
dir="${GIT_HYDRA_DIR:?}"
gitdir=$(dirname "$dir")
id=$(tail -n 1 "$gitdir/rebase-merge/done" 2>/dev/null | awk '{print $2}')
[ -n "$id" ] || exit 0
while IFS= read -r key && IFS= read -r file; do
  case "$key" in
    "$id"*)
      if [ -f "$file" ]; then
        cat "$file" > "$1"
        # When git cannot make the commit (a hook refused it, a signing tool did not answer),
        # --continue commits again from this file, not from what the editor wrote.
        [ -f "$gitdir/rebase-merge/message" ] && cat "$file" > "$gitdir/rebase-merge/message"
      fi
      exit 0
      ;;
  esac
done < "$dir/messages"
exit 0
"#;

/// Finishes a `reword` whose new message was refused the first time (a hook, a signing tool that
/// did not answer). git stops there and, on *Continue*, goes on with the old message. So the
/// commit is amended here with the message chosen in the app (or, when the changes were left
/// staged, made with it, keeping the original author and date); *Continue* then carries on.
/// Does nothing in any other state.
pub(super) fn commit_chosen_reword(ctx: &OpCtx) -> Result<(), GitError> {
    let dir = helper_dir(ctx)?;
    let Some(git_dir) = dir.parent() else {
        return Ok(());
    };
    let (Ok(done), Ok(messages)) = (
        fs::read_to_string(git_dir.join("rebase-merge/done")),
        fs::read_to_string(dir.join("messages")),
    ) else {
        return Ok(());
    };
    let mut command = done.lines().last().unwrap_or_default().split_whitespace();
    let (Some("reword"), Some(id)) = (command.next(), command.next()) else {
        return Ok(());
    };
    let mut lines = messages.lines();
    let mut chosen = None;
    while let (Some(key), Some(file)) = (lines.next(), lines.next()) {
        if key.starts_with(id) {
            chosen = Some(file.to_string());
            break;
        }
    }
    let Some(file) = chosen else {
        return Ok(());
    };
    let status = ctx.status()?;
    if !status.conflicted.is_empty() {
        return Ok(());
    }
    if status.staged.is_empty() {
        // The picked commit exists and only its new message was refused: amend it.
        ctx.runner().run(
            ctx.workdir,
            &["commit", "--amend", "--allow-empty", "-F", &file],
            RunOptions::with_env(fallback_identity(ctx)),
        )?;
        return Ok(());
    }
    let author = ctx.runner().run(
        ctx.workdir,
        &["log", "-1", "--format=%an%x1f%ae%x1f%aI", id],
        RunOptions::background(),
    )?;
    let text = author.stdout_text();
    let mut parts = text.trim().split('\x1f');
    let (Some(name), Some(email), Some(date)) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(());
    };
    let mut env = fallback_identity(ctx);
    env.extend([
        ("GIT_AUTHOR_NAME".to_string(), name.to_string()),
        ("GIT_AUTHOR_EMAIL".to_string(), email.to_string()),
        ("GIT_AUTHOR_DATE".to_string(), date.to_string()),
    ]);
    ctx.runner().run(
        ctx.workdir,
        &["commit", "--allow-empty", "-F", &file],
        RunOptions::with_env(env),
    )?;
    Ok(())
}

/// The environment that lets a rebase started here find its message editor again when it is
/// continued, skipped or restarted after a stop. Empty when this app did not start one.
pub(super) fn rebase_env(ctx: &OpCtx) -> Result<Vec<(String, String)>, GitError> {
    let dir = helper_dir(ctx)?;
    let editor = dir.join("message-editor.sh");
    if !editor.exists() {
        return Ok(Vec::new());
    }
    Ok(vec![
        ("GIT_EDITOR".to_string(), shell_quote(&editor)),
        (
            "GIT_HYDRA_DIR".to_string(),
            dir.to_string_lossy().into_owned(),
        ),
    ])
}

/// Rebases the current branch onto `onto`. With `items` the list of commits is the one given
/// (interactive rebase); without, git replays them all as they are.
///
/// Conflicts, and a stop for `edit`, are not errors: the rebase stays in progress and the
/// outcome says so, for the banner and the conflict resolver to take over.
pub fn rebase(
    ctx: &OpCtx,
    onto: &str,
    items: Option<&[TodoItem]>,
    autostash: bool,
) -> Result<RebaseResult, GitError> {
    check_target(onto)?;
    let _write = ctx.stores.begin_write()?;
    let status = ctx.status()?;
    if status.operation.is_some() || !status.conflicted.is_empty() {
        return Err(GitError::other(
            "Another operation is waiting to be finished or cancelled: do that first.",
        ));
    }
    if status.detached {
        return Err(GitError::other(
            "HEAD is detached: switch to a branch before rebasing it.",
        ));
    }
    let before = head_id(ctx)?.ok_or_else(|| GitError::other("There is no commit yet."))?;
    let plan = rebase_plan(ctx, onto)?;

    let mut env = fallback_identity(ctx);
    let mut args = vec!["rebase"];
    if let Some(items) = items {
        env.extend(prepare_todo(ctx, &plan, items)?);
        args.push("--interactive");
    }
    if autostash {
        args.push("--autostash");
    }
    args.push(onto);

    let output = ctx
        .runner()
        .run_raw(ctx.workdir, &args, RunOptions::with_env(env))?;
    let status = ctx.status()?;
    let in_progress = status
        .operation
        .as_ref()
        .is_some_and(|op| op.kind == OperationKind::Rebase);
    if output.code != 0 {
        // Conflicts are what the interface resolves. A rebase that stopped for another reason
        // (a commit that is now empty, a hook, a signing tool that refused) is reported with what
        // git said; it stays in progress and the banner offers to continue, skip or abort.
        if in_progress && !status.conflicted.is_empty() {
            return Ok(RebaseResult {
                outcome: RebaseOutcome::Conflicts,
                status,
                before: before.clone(),
                after: before,
            });
        }
        return Err(error_from(&output));
    }
    let after = head_id(ctx)?.unwrap_or_else(|| before.clone());
    let said = format!("{}{}", output.stdout_text(), output.stderr);
    let outcome = if in_progress {
        if status.conflicted.is_empty() {
            RebaseOutcome::Stopped
        } else {
            RebaseOutcome::Conflicts
        }
    } else if said.contains("is up to date") && before == after {
        RebaseOutcome::UpToDate
    } else {
        RebaseOutcome::Done
    };
    Ok(RebaseResult {
        status,
        outcome,
        before,
        after,
    })
}
