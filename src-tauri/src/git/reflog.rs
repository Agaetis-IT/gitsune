//! The reflog of HEAD as a history of positions, with a way back to any of them.

use serde::Serialize;

use super::cli::{GitError, RunOptions};
use super::ops::{OpCtx, head_id};
use super::status::WorkingStatus;

/// Entries read from the reflog.
const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReflogEntry {
    /// The commit HEAD pointed at after the operation.
    pub id: String,
    /// What git recorded, e.g. `commit: Add a file` or `checkout: moving from a to b`.
    pub message: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    /// HEAD points at this commit now.
    pub current: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub status: WorkingStatus,
    /// What was undone, for the message shown to the user.
    pub undone: String,
}

/// What undoing an entry means.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Move the current branch (or detached HEAD) back to this commit.
    Reset(String),
    /// Go back to this branch or commit.
    Switch(String),
}

pub fn list_reflog(ctx: &OpCtx) -> Result<Vec<ReflogEntry>, GitError> {
    let head = head_id(ctx)?;
    Ok(read_entries(ctx)?
        .into_iter()
        .enumerate()
        .map(|(index, mut entry)| {
            entry.current = index == 0 && head.as_deref() == Some(entry.id.as_str());
            entry
        })
        .collect())
}

fn read_entries(ctx: &OpCtx) -> Result<Vec<ReflogEntry>, GitError> {
    let limit = format!("-n{MAX_ENTRIES}");
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &[
            "reflog",
            "show",
            "--format=%H%x1f%ct%x1f%gs",
            &limit,
            "HEAD",
            "--",
        ],
        RunOptions::background(),
    )?;
    if output.code != 0 {
        // A repository without a commit has no reflog yet.
        return Ok(Vec::new());
    }
    Ok(parse_entries(&output.stdout_text()))
}

fn parse_entries(text: &str) -> Vec<ReflogEntry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\u{1f}');
            let id = parts.next()?.to_string();
            let time = parts.next()?.parse().ok()?;
            let message = parts.next()?.to_string();
            (!id.is_empty()).then_some(ReflogEntry {
                id,
                message,
                time,
                current: false,
            })
        })
        .collect()
}

/// What to do to take back the most recent entry of `entries` (newest first).
fn plan_undo(entries: &[ReflogEntry]) -> Option<(Step, String)> {
    let latest = entries.first()?;
    let message = latest.message.as_str();
    if let Some(rest) = message.strip_prefix("checkout: moving from ") {
        let from = rest.rsplit_once(" to ")?.0;
        return Some((Step::Switch(from.to_string()), "the checkout".into()));
    }
    // A rebase writes one entry per replayed commit: go back to before it started.
    let mut skip = 1;
    if message.starts_with("rebase")
        && let Some(start) = entries
            .iter()
            .position(|entry| entry.message.starts_with("rebase (start)"))
    {
        skip = start + 1;
    }
    let target = entries.get(skip)?;
    let what = message.split(':').next().unwrap_or(message).to_string();
    Some((Step::Reset(target.id.clone()), format!("the {what}")))
}

/// Moves the current branch back to a commit of the reflog. Changes that would be
/// overwritten make git refuse (`--keep`), so nothing is lost.
pub fn restore_head(ctx: &OpCtx, target: &str) -> Result<WorkingStatus, GitError> {
    let _write = ctx.stores.begin_write()?;
    reset_keep(ctx, target)?;
    ctx.status()
}

fn reset_keep(ctx: &OpCtx, target: &str) -> Result<(), GitError> {
    ctx.runner().run(
        ctx.workdir,
        &["reset", "--keep", target],
        RunOptions::default(),
    )?;
    Ok(())
}

/// Takes back the last thing that moved HEAD. Running it again takes the undo back (redo).
pub fn undo_last(ctx: &OpCtx) -> Result<UndoResult, GitError> {
    let _write = ctx.stores.begin_write()?;
    let entries = read_entries(ctx)?;
    let Some((step, undone)) = plan_undo(&entries) else {
        return Err(GitError::other("There is nothing to undo."));
    };
    match step {
        Step::Reset(target) => reset_keep(ctx, &target)?,
        Step::Switch(name) => {
            let is_branch = ctx
                .runner()
                .run_raw(
                    ctx.workdir,
                    &["show-ref", "--verify", "-q", &format!("refs/heads/{name}")],
                    RunOptions::background(),
                )?
                .code
                == 0;
            let args: Vec<&str> = if is_branch {
                vec!["switch", &name]
            } else {
                vec!["checkout", "--detach", &name]
            };
            ctx.runner()
                .run(ctx.workdir, &args, RunOptions::default())?;
        }
    }
    Ok(UndoResult {
        status: ctx.status()?,
        undone,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn entry(message: &str) -> ReflogEntry {
        ReflogEntry {
            id: message.len().to_string(),
            message: message.into(),
            time: 0,
            current: false,
        }
    }

    #[test]
    fn a_checkout_is_undone_by_switching_back() {
        let entries = [
            entry("checkout: moving from main to feat/x"),
            entry("commit: a"),
        ];
        let (step, _) = plan_undo(&entries).unwrap();
        assert_eq!(step, Step::Switch("main".into()));
    }

    #[test]
    fn a_rebase_goes_back_to_before_it_started() {
        let entries = [
            entry("rebase (finish): returning to refs/heads/x"),
            entry("rebase (pick): two"),
            entry("rebase (start): checkout main"),
            entry("commit: original"),
        ];
        let (step, _) = plan_undo(&entries).unwrap();
        assert_eq!(step, Step::Reset("commit: original".len().to_string()));
    }

    #[test]
    fn nothing_to_undo_with_a_single_entry() {
        assert!(plan_undo(&[entry("commit (initial): a")]).is_none());
    }
}
