//! The state of the working tree and the index, from `git status --porcelain=v2`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::cli::{GitError, RunOptions, Runner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileState {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    /// Not tracked by git (shown with the unstaged changes).
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub path: String,
    /// Previous path of a renamed or copied file.
    pub old_path: Option<String>,
    pub state: FileState,
    /// The entry is a submodule.
    pub submodule: bool,
}

/// A file with unresolved conflicts (resolving them comes with merges, M14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub path: String,
    /// The two-letter code of git (`UU`, `AA`, `DU`...).
    pub code: String,
}

/// What a merge, cherry-pick, revert or rebase that is not finished is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

/// An operation waiting for the user, found from the git directory (so also when it was
/// started in a terminal, and after the app is restarted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub kind: OperationKind,
    /// The first line of the message git prepared (`Merge branch 'x'`), when there is one.
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingStatus {
    /// A merge, cherry-pick, revert or rebase that is not finished.
    pub operation: Option<Operation>,
    /// Checked-out branch, `None` when HEAD is detached.
    pub branch: Option<String>,
    pub detached: bool,
    /// Commit HEAD points at, `None` in a repository without commits.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Changes in the index (what the next commit will contain).
    pub staged: Vec<StatusEntry>,
    /// Changes in the working tree not in the index, untracked files included.
    pub unstaged: Vec<StatusEntry>,
    pub conflicted: Vec<ConflictEntry>,
}

fn state_of(code: char) -> Option<FileState> {
    match code {
        'A' => Some(FileState::Added),
        'M' => Some(FileState::Modified),
        'D' => Some(FileState::Deleted),
        'R' => Some(FileState::Renamed),
        'C' => Some(FileState::Copied),
        'T' => Some(FileState::TypeChanged),
        _ => None,
    }
}

/// Parses the output of `git status --porcelain=v2 --branch -z`.
pub fn parse_status(output: &[u8]) -> WorkingStatus {
    let mut status = WorkingStatus::default();
    let text = String::from_utf8_lossy(output);
    let mut records = text.split('\0').filter(|r| !r.is_empty());

    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            parse_header(header, &mut status);
            continue;
        }
        let kind = record.as_bytes()[0];
        match kind {
            b'1' | b'2' => {
                let fields: Vec<&str> = record
                    .splitn(if kind == b'1' { 9 } else { 10 }, ' ')
                    .collect();
                let (Some(xy), Some(sub), Some(path)) =
                    (fields.get(1), fields.get(2), fields.last())
                else {
                    continue;
                };
                let old_path = (kind == b'2')
                    .then(|| records.next().map(str::to_string))
                    .flatten();
                let mut chars = xy.chars();
                let (staged, unstaged) = (
                    chars.next().and_then(state_of),
                    chars.next().and_then(state_of),
                );
                let entry = |state| StatusEntry {
                    path: (*path).to_string(),
                    old_path: old_path.clone(),
                    state,
                    submodule: sub.starts_with('S'),
                };
                if let Some(state) = staged {
                    status.staged.push(entry(state));
                }
                if let Some(state) = unstaged {
                    status.unstaged.push(entry(state));
                }
            }
            b'u' => {
                let fields: Vec<&str> = record.splitn(11, ' ').collect();
                if let (Some(code), Some(path)) = (fields.get(1), fields.last()) {
                    status.conflicted.push(ConflictEntry {
                        path: (*path).to_string(),
                        code: (*code).to_string(),
                    });
                }
            }
            b'?' => {
                if let Some(path) = record.strip_prefix("? ") {
                    status.unstaged.push(StatusEntry {
                        path: path.to_string(),
                        old_path: None,
                        state: FileState::Untracked,
                        submodule: false,
                    });
                }
            }
            _ => {}
        }
    }
    status.staged.sort_by(|a, b| a.path.cmp(&b.path));
    status.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    status.conflicted.sort_by(|a, b| a.path.cmp(&b.path));
    status
}

fn parse_header(header: &str, status: &mut WorkingStatus) {
    if let Some(oid) = header.strip_prefix("branch.oid ") {
        status.head = (oid != "(initial)").then(|| oid.to_string());
    } else if let Some(head) = header.strip_prefix("branch.head ") {
        status.detached = head == "(detached)";
        status.branch = (!status.detached).then(|| head.to_string());
    } else if let Some(upstream) = header.strip_prefix("branch.upstream ") {
        status.upstream = Some(upstream.to_string());
    } else if let Some(counts) = header.strip_prefix("branch.ab ") {
        let mut parts = counts.split(' ');
        status.ahead = parts
            .next()
            .and_then(|a| a.trim_start_matches('+').parse().ok())
            .unwrap_or(0);
        status.behind = parts
            .next()
            .and_then(|b| b.trim_start_matches('-').parse().ok())
            .unwrap_or(0);
    }
}

/// The git directory of a working tree (its `.git`, or what a `.git` file points to).
fn git_dir(runner: &Runner, workdir: &Path) -> Option<PathBuf> {
    let dot = workdir.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let output = runner
        .run(
            workdir,
            &["rev-parse", "--absolute-git-dir"],
            RunOptions::background(),
        )
        .ok()?;
    Some(PathBuf::from(output.stdout_text().trim()))
}

fn first_line(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
        .unwrap_or_default()
}

/// "Commit 2 of 5" for a rebase that is under way.
fn rebase_progress(dir: &Path) -> String {
    for folder in ["rebase-merge", "rebase-apply"] {
        let (done, total) = if folder == "rebase-merge" {
            ("msgnum", "end")
        } else {
            ("next", "last")
        };
        let read = |name: &str| first_line(&dir.join(folder).join(name));
        let (done, total) = (read(done), read(total));
        if !done.is_empty() && !total.is_empty() {
            return format!("Commit {done} of {total}");
        }
    }
    String::new()
}

/// Detects an operation in progress from the files git keeps while it waits.
pub fn read_operation(runner: &Runner, workdir: &Path) -> Option<Operation> {
    let dir = git_dir(runner, workdir)?;
    let has = |name: &str| dir.join(name).exists();
    let (kind, message) = if has("rebase-merge") || has("rebase-apply") {
        (OperationKind::Rebase, rebase_progress(&dir))
    } else if has("MERGE_HEAD") {
        (OperationKind::Merge, first_line(&dir.join("MERGE_MSG")))
    } else if has("CHERRY_PICK_HEAD") {
        (OperationKind::CherryPick, String::new())
    } else if has("REVERT_HEAD") {
        (OperationKind::Revert, String::new())
    } else {
        return None;
    };
    Some(Operation { kind, message })
}

/// Reads the status of the working tree. It never takes optional locks and is not logged: it
/// runs in the background each time something changes.
pub fn read_status(runner: &Runner, workdir: &Path) -> Result<WorkingStatus, GitError> {
    let output = runner.run(
        workdir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
        ],
        RunOptions::background(),
    )?;
    let mut status = parse_status(&output.stdout);
    status.operation = read_operation(runner, workdir);
    Ok(status)
}
