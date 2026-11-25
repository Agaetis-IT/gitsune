//! Topological ordering of a commit graph (children before parents).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A commit as read from the repository, before ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCommit {
    /// Indices of the parents in the input slice.
    pub parents: Vec<usize>,
    /// Commit time in seconds since the Unix epoch.
    pub time: i64,
}

/// Returns input indices ordered so every commit comes before all its parents.
///
/// Among the commits that are ready, the newest is emitted first; ties are broken
/// by input index so the result is deterministic. Unlike a plain sort by date,
/// this stays correct when a parent has a newer timestamp than its child (clock skew).
pub fn topological_order(commits: &[RawCommit]) -> Vec<usize> {
    let mut pending_children = vec![0u32; commits.len()];
    for commit in commits {
        for &parent in &commit.parents {
            pending_children[parent] += 1;
        }
    }

    let mut ready: BinaryHeap<(i64, Reverse<usize>)> = pending_children
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(index, _)| (commits[index].time, Reverse(index)))
        .collect();

    let mut order = Vec::with_capacity(commits.len());
    while let Some((_, Reverse(index))) = ready.pop() {
        order.push(index);
        for &parent in &commits[index].parents {
            pending_children[parent] -= 1;
            if pending_children[parent] == 0 {
                ready.push((commits[parent].time, Reverse(parent)));
            }
        }
    }
    order
}
