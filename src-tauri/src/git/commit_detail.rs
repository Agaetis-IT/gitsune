//! Details of one commit: its message and changed files, and the diff of a single file.

use std::collections::BTreeMap;

use gix::object::tree::EntryKind;
use gix::{ObjectId, ThreadSafeRepository};
use serde::Serialize;

use super::text_diff::{self, Hunk};

/// Largest blob (either side) whose diff is computed without being asked.
pub const MAX_AUTO_BLOB_BYTES: u64 = 1024 * 1024;
/// Largest blob whose diff can be computed at all, when the user insists.
pub const MAX_FORCED_BLOB_BYTES: u64 = 8 * 1024 * 1024;
/// Longest diff sent to the frontend, in lines.
pub const MAX_DIFF_LINES: usize = 5000;
/// Files for which line counts are computed in the file list.
const MAX_STAT_FILES: usize = 300;
/// Files listed for one commit.
const MAX_LISTED_FILES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// Previous path of a renamed or copied file.
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    /// Added and removed lines; `None` when not counted (binary, large, or too many files).
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    /// Known to be binary (only checked for the files whose lines were counted).
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub id: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_time: i64,
    /// Full commit message.
    pub message: String,
    /// Changes against the first parent (against nothing for a root commit), sorted by path.
    pub files: Vec<FileChange>,
    /// The commit changes more files than `files` lists.
    pub files_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    pub binary: bool,
    /// The file is a submodule: only its commit id changed.
    pub submodule: bool,
    /// A blob is bigger than the limit, so no diff was computed.
    pub too_large: bool,
    /// `too_large` can be overridden by asking again with `force`.
    pub can_force: bool,
    /// The diff was cut at the line limit.
    pub truncated: bool,
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
}

fn parse_id(id: &str) -> Result<ObjectId, String> {
    ObjectId::from_hex(id.as_bytes()).map_err(|e| format!("bad commit id {id}: {e}"))
}

/// A side of a change: the blob id and what kind of entry it is.
#[derive(Clone, Copy)]
struct Side {
    id: ObjectId,
    kind: EntryKind,
}

impl Side {
    fn is_submodule(self) -> bool {
        self.kind == EntryKind::Commit
    }
    fn is_regular(self) -> bool {
        matches!(self.kind, EntryKind::Blob | EntryKind::BlobExecutable)
    }
}

fn status_for(old: Option<Side>, new: Option<Side>, renamed: Option<bool>) -> ChangeStatus {
    match (old, new) {
        (None, Some(_)) => ChangeStatus::Added,
        (Some(_), None) => ChangeStatus::Deleted,
        (Some(a), Some(b)) => match renamed {
            Some(true) => ChangeStatus::Copied,
            Some(false) => ChangeStatus::Renamed,
            None if a.is_regular() != b.is_regular() || a.is_submodule() != b.is_submodule() => {
                ChangeStatus::TypeChanged
            }
            None => ChangeStatus::Modified,
        },
        (None, None) => ChangeStatus::Modified,
    }
}

/// Tree of the first parent, or the empty tree for a root commit.
fn first_parent_tree<'r>(
    repo: &'r gix::Repository,
    commit: &gix::Commit<'r>,
    id: &str,
) -> Result<gix::Tree<'r>, String> {
    match commit.parent_ids().next() {
        Some(parent) => repo
            .find_commit(parent)
            .map_err(|e| format!("cannot read parent of {id}: {e}"))?
            .tree()
            .map_err(|e| format!("cannot read parent of {id}: {e}")),
        None => Ok(repo.empty_tree()),
    }
}

/// Reads a commit with the list of files it changes.
pub fn commit_detail(repo: &ThreadSafeRepository, id: &str) -> Result<CommitDetail, String> {
    let repo = repo.to_thread_local();
    let oid = parse_id(id)?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| format!("cannot read commit {id}: {e}"))?;
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let parent_ids: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
    let parent_tree = first_parent_tree(&repo, &commit, id)?;

    let changes = repo
        .diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)
        .map_err(|e| format!("cannot diff {id}: {e}"))?;

    let mut files: BTreeMap<String, FileChange> = BTreeMap::new();
    let mut sides: BTreeMap<String, (Option<Side>, Option<Side>)> = BTreeMap::new();
    for change in changes {
        use gix::object::tree::diff::ChangeDetached as C;
        let (path, old_path, old, new, renamed) = match change {
            C::Addition {
                location,
                entry_mode,
                id,
                ..
            } => (
                location.to_string(),
                None,
                None,
                Some((entry_mode, id)),
                None,
            ),
            C::Deletion {
                location,
                entry_mode,
                id,
                ..
            } => (
                location.to_string(),
                None,
                Some((entry_mode, id)),
                None,
                None,
            ),
            C::Modification {
                location,
                previous_entry_mode,
                previous_id,
                entry_mode,
                id,
            } => (
                location.to_string(),
                None,
                Some((previous_entry_mode, previous_id)),
                Some((entry_mode, id)),
                None,
            ),
            C::Rewrite {
                source_location,
                source_entry_mode,
                source_id,
                entry_mode,
                id,
                location,
                copy,
                ..
            } => (
                location.to_string(),
                Some(source_location.to_string()),
                Some((source_entry_mode, source_id)),
                Some((entry_mode, id)),
                Some(copy),
            ),
        };
        let side = |(mode, id): (gix::object::tree::EntryMode, ObjectId)| Side {
            id,
            kind: mode.kind(),
        };
        let (old, new) = (old.map(side), new.map(side));
        // Directories themselves are not files.
        if [old, new]
            .into_iter()
            .flatten()
            .any(|s| s.kind == EntryKind::Tree)
        {
            continue;
        }
        sides.insert(path.clone(), (old, new));
        files.insert(
            path.clone(),
            FileChange {
                path,
                old_path,
                status: status_for(old, new, renamed),
                additions: None,
                deletions: None,
                binary: false,
            },
        );
    }

    let truncated = files.len() > MAX_LISTED_FILES;
    let mut list: Vec<FileChange> = files.into_values().take(MAX_LISTED_FILES).collect();
    if list.len() <= MAX_STAT_FILES {
        for file in &mut list {
            let (old, new) = sides[&file.path];
            fill_stats(&repo, file, old, new);
        }
    }

    let author = commit.author().map_err(|e| e.to_string())?;
    let message = String::from_utf8_lossy(commit.message_raw_sloppy())
        .trim_end()
        .to_string();
    Ok(CommitDetail {
        id: oid.to_string(),
        parents: parent_ids.iter().map(ToString::to_string).collect(),
        author_name: author.name.to_string(),
        author_email: author.email.to_string(),
        author_time: author.time().map(|t| t.seconds).unwrap_or_default(),
        message,
        files: list,
        files_truncated: truncated,
    })
}

/// Fills in line counts and the binary flag when both sides are small regular files.
fn fill_stats(repo: &gix::Repository, file: &mut FileChange, old: Option<Side>, new: Option<Side>) {
    let usable = |side: Option<Side>| side.is_none_or(|s| s.is_regular());
    if !usable(old) || !usable(new) {
        return;
    }
    let (Some(old_data), Some(new_data)) = (
        read_blob(repo, old, MAX_AUTO_BLOB_BYTES),
        read_blob(repo, new, MAX_AUTO_BLOB_BYTES),
    ) else {
        return;
    };
    if text_diff::is_binary(&old_data) || text_diff::is_binary(&new_data) {
        file.binary = true;
        return;
    }
    let (additions, deletions) = text_diff::line_stats(&old_data, &new_data);
    file.additions = Some(additions);
    file.deletions = Some(deletions);
}

/// The content of a side (empty when absent), or `None` if it is bigger than `limit`.
fn read_blob(repo: &gix::Repository, side: Option<Side>, limit: u64) -> Option<Vec<u8>> {
    let Some(side) = side else {
        return Some(Vec::new());
    };
    if repo.find_header(side.id).ok()?.size() > limit {
        return None;
    }
    Some(repo.find_blob(side.id).ok()?.detach().data)
}

/// Diff of one file of a commit against its first parent.
///
/// `old_path` is the previous path for a renamed file. Diffs of blobs over
/// [`MAX_AUTO_BLOB_BYTES`] are only computed with `force` (up to [`MAX_FORCED_BLOB_BYTES`]).
pub fn file_diff(
    repo: &ThreadSafeRepository,
    id: &str,
    path: &str,
    old_path: Option<&str>,
    force: bool,
) -> Result<FileDiff, String> {
    let repo = repo.to_thread_local();
    let oid = parse_id(id)?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| format!("cannot read commit {id}: {e}"))?;
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let parent_tree = first_parent_tree(&repo, &commit, id)?;

    let lookup = |tree: &gix::Tree<'_>, at: &str| -> Result<Option<Side>, String> {
        let entry = tree.lookup_entry_by_path(at).map_err(|e| e.to_string())?;
        Ok(entry.map(|e| Side {
            id: e.object_id(),
            kind: e.mode().kind(),
        }))
    };
    let new = lookup(&tree, path)?;
    let old_at = old_path.unwrap_or(path);
    let old = lookup(&parent_tree, old_at)?;
    if old.is_none() && new.is_none() {
        return Err(format!("{path} is not changed by {id}"));
    }
    let renamed = (old_at != path).then_some(false);

    let mut result = FileDiff {
        path: path.to_string(),
        old_path: (old_at != path).then(|| old_at.to_string()),
        status: status_for(old, new, renamed),
        binary: false,
        submodule: old.is_some_and(Side::is_submodule) || new.is_some_and(Side::is_submodule),
        too_large: false,
        can_force: false,
        truncated: false,
        old_size: size_of(&repo, old),
        new_size: size_of(&repo, new),
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
    };
    if result.submodule {
        return Ok(result);
    }

    let biggest = result
        .old_size
        .unwrap_or(0)
        .max(result.new_size.unwrap_or(0));
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

    let (Some(old_data), Some(new_data)) =
        (read_blob(&repo, old, limit), read_blob(&repo, new, limit))
    else {
        return Err(format!("cannot read the content of {path}"));
    };
    if text_diff::is_binary(&old_data) || text_diff::is_binary(&new_data) {
        result.binary = true;
        return Ok(result);
    }
    let diff = text_diff::text_diff(&old_data, &new_data, MAX_DIFF_LINES);
    result.additions = diff.additions;
    result.deletions = diff.deletions;
    result.truncated = diff.truncated;
    result.hunks = diff.hunks;
    Ok(result)
}

fn size_of(repo: &gix::Repository, side: Option<Side>) -> Option<u64> {
    repo.find_header(side?.id).ok().map(|header| header.size())
}
