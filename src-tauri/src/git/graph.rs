use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use gix::ObjectId;
use gix::ThreadSafeRepository;

use super::refs::{RefInfo, RefKind};
use super::store::{CommitStore, StoredCommit};
use crate::layout::{self, RawCommit};

/// The commit graph of a repository, in layout order (children before parents).
pub struct CommitGraph {
    /// Commit ids, indexed by row.
    pub ids: Vec<ObjectId>,
    pub commits: Vec<layout::Commit>,
    pub refs: Vec<layout::RefLabel>,
}

/// Reads every commit reachable from `refs` and puts them in topological order.
///
/// Commits already in `store` are not read again. The result only depends on the commits
/// and the refs, never on what the store held before: commits are put in a canonical order
/// (by id) before the topological sort, so ties between equal timestamps always break the same way.
pub fn load_graph(
    repo: &ThreadSafeRepository,
    refs: &[RefInfo],
    store: &mut CommitStore,
) -> Result<CommitGraph, String> {
    let repo = repo.to_thread_local();

    let mut tips = Vec::with_capacity(refs.len());
    for reference in refs {
        let id = ObjectId::from_hex(reference.target.as_bytes())
            .map_err(|e| format!("bad id for {}: {e}", reference.full_name))?;
        tips.push(id);
    }

    // A stash is a merge of the commit it was made on with the index and, with untracked files,
    // a third commit. Only the first parent, the commit it was made on, belongs to the history.
    let stash_tips: HashSet<ObjectId> = refs
        .iter()
        .zip(&tips)
        .filter(|(reference, _)| reference.kind == RefKind::Stash)
        .map(|(_, tip)| *tip)
        .collect();
    let followed = |id: &ObjectId, parents: &'_ [ObjectId]| -> usize {
        if stash_tips.contains(id) {
            parents.len().min(1)
        } else {
            parents.len()
        }
    };

    // Walk the parents of every tip, reading only commits the store does not know yet.
    // Unreadable commits (the missing parents of a shallow clone) are left out.
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut ids: Vec<ObjectId> = Vec::new();
    let mut stack = tips.clone();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Entry::Vacant(slot) = store.commits.entry(id) {
            match read_commit(&repo, id) {
                Ok(commit) => {
                    slot.insert(commit);
                }
                Err(_) => continue,
            }
        }
        ids.push(id);
        let parents = &store.commits[&id].parents;
        stack.extend(
            parents[..followed(&id, parents)]
                .iter()
                .filter(|p| !seen.contains(*p)),
        );
    }
    ids.sort_unstable();
    let times: Vec<i64> = ids.iter().map(|id| store.commits[id].time).collect();
    let parent_ids: Vec<&[ObjectId]> = ids
        .iter()
        .map(|id| {
            let parents = &store.commits[id].parents;
            &parents[..followed(id, parents)]
        })
        .collect();
    let mut subjects: Vec<String> = ids
        .iter()
        .map(|id| store.commits[id].subject.clone())
        .collect();

    // Parents missing from the walk (shallow clones) are dropped.
    let index_of: HashMap<ObjectId, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let raw: Vec<RawCommit> = parent_ids
        .iter()
        .zip(&times)
        .map(|(parents, &time)| RawCommit {
            parents: parents
                .iter()
                .filter_map(|p| index_of.get(p).copied())
                .collect(),
            time,
        })
        .collect();

    let order = layout::topological_order(&raw);
    let mut row_of = vec![0usize; ids.len()];
    for (row, &index) in order.iter().enumerate() {
        row_of[index] = row;
    }

    let commits = order
        .iter()
        .map(|&index| layout::Commit {
            parents: raw[index].parents.iter().map(|&p| row_of[p]).collect(),
            subject: std::mem::take(&mut subjects[index]),
        })
        .collect();
    let sorted_ids = order.iter().map(|&index| ids[index]).collect();

    let labels = refs
        .iter()
        .zip(&tips)
        .filter_map(|(reference, tip)| {
            Some(layout::RefLabel {
                row: row_of[*index_of.get(tip)?],
                name: reference.name.clone(),
                kind: match reference.kind {
                    RefKind::Local => layout::RefKind::Local,
                    RefKind::Remote => layout::RefKind::Remote,
                    RefKind::Tag => layout::RefKind::Tag,
                    RefKind::Stash => layout::RefKind::Stash,
                },
                is_head: reference.is_head,
            })
        })
        .collect();

    Ok(CommitGraph {
        ids: sorted_ids,
        commits,
        refs: labels,
    })
}

fn read_commit(repo: &gix::Repository, id: ObjectId) -> Result<StoredCommit, String> {
    let commit = repo.find_commit(id).map_err(|e| e.to_string())?;
    let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
    let subject = if parents.len() > 1 {
        first_line(commit.message_raw_sloppy())
    } else {
        String::new()
    };
    let time = commit.time().map(|t| t.seconds).unwrap_or_default();
    Ok(StoredCommit {
        parents,
        time,
        subject,
    })
}

pub(super) fn first_line(message: &[u8]) -> String {
    let end = message
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(message.len());
    String::from_utf8_lossy(&message[..end]).trim().to_string()
}
