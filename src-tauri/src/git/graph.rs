use std::collections::HashMap;

use gix::ObjectId;
use gix::ThreadSafeRepository;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use super::refs::{RefInfo, RefKind};
use crate::layout::{self, RawCommit};

/// The commit graph of a repository, in layout order (children before parents).
pub struct CommitGraph {
    /// Commit ids, indexed by row. Read by the paged graph API (M3) and the tests.
    #[allow(dead_code)]
    pub ids: Vec<ObjectId>,
    pub commits: Vec<layout::Commit>,
    pub refs: Vec<layout::RefLabel>,
}

/// Reads every commit reachable from `refs` and puts them in topological order.
pub fn load_graph(repo: &ThreadSafeRepository, refs: &[RefInfo]) -> Result<CommitGraph, String> {
    let repo = repo.to_thread_local();

    let mut tips = Vec::with_capacity(refs.len());
    for reference in refs {
        let id = ObjectId::from_hex(reference.target.as_bytes())
            .map_err(|e| format!("bad id for {}: {e}", reference.full_name))?;
        tips.push(id);
    }

    // Read the whole graph once: id, parents, time (and the subject of merge commits only,
    // since that is all the layout needs).
    let walk = repo
        .rev_walk(tips.clone())
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(|e| e.to_string())?;

    let mut ids: Vec<ObjectId> = Vec::new();
    let mut parent_ids: Vec<Vec<ObjectId>> = Vec::new();
    let mut times: Vec<i64> = Vec::new();
    let mut subjects: Vec<String> = Vec::new();
    for info in walk {
        let info = info.map_err(|e| e.to_string())?;
        let parents: Vec<ObjectId> = info.parent_ids().map(|p| p.detach()).collect();
        let subject = if parents.len() > 1 {
            let commit = repo.find_commit(info.id).map_err(|e| e.to_string())?;
            first_line(commit.message_raw_sloppy())
        } else {
            String::new()
        };
        ids.push(info.id);
        times.push(info.commit_time());
        parent_ids.push(parents);
        subjects.push(subject);
    }

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

fn first_line(message: &[u8]) -> String {
    let end = message
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(message.len());
    String::from_utf8_lossy(&message[..end]).trim().to_string()
}
