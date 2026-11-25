//! Lane layout for the metro graph.
//!
//! Input: commits in topological order (row 0 is the newest; every parent has a larger
//! row index than its children) plus the refs pointing at them.
//!
//! Output: for every row the column of its commit, plus the lanes (one per branch line)
//! and the connectors that are not straight vertical lines.
//!
//! Rules:
//! - A commit's first parent stays on the commit's lane (same column).
//! - Every other parent starts a new lane in the nearest free column.
//! - A lane keeps its column for its whole life. When several lanes point at the same
//!   commit they all run down to it and converge there; the leftmost one carries the commit.
//! - A lane is named after the best ref found on it, else after the merge commit message
//!   that started it, else it is anonymous. Its color only depends on that name.

mod naming;
mod topo;

use std::collections::HashMap;

use serde::Serialize;

pub use naming::merge_branch_name;
pub use topo::{RawCommit, topological_order};

/// A commit in layout order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Row indices of the parents (all greater than this commit's row).
    pub parents: Vec<usize>,
    /// First line of the message. Only read for merge commits (to name merged lanes).
    pub subject: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

/// A ref pointing at a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub row: usize,
    pub name: String,
    pub kind: RefKind,
    pub is_head: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaneKind {
    /// Named after a ref (branch or, as a last resort, tag).
    Branch,
    /// No ref left: named after the merge commit that started it.
    Merged,
    Anonymous,
}

/// One branch line. It occupies `col` from `first_row` to `last_row`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    pub id: u32,
    pub col: u32,
    /// Row where the lane begins (its tip commit, or the merge commit that forked it).
    pub first_row: usize,
    /// Row where the lane ends (its last commit, or the commit it converges into).
    pub last_row: usize,
    /// The lane begins as a connector leaving a merge commit (it has a `Start` edge in
    /// `first_row`); otherwise it begins at its own tip commit in `first_row`.
    pub forked: bool,
    /// The lane ends by bending into another commit's lane (an `End` edge in the span
    /// before `last_row`); otherwise it ends at its own last commit in `last_row`.
    pub joins: bool,
    pub name: Option<String>,
    pub kind: LaneKind,
    /// Palette index, `None` for anonymous lanes (drawn in a neutral color).
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bend {
    /// The column change happens right after the commit (a lane forking away).
    Start,
    /// The column change happens right before the next row (a lane converging).
    End,
}

/// A connector between a row and the next one that is not a straight vertical line.
///
/// Spans without an edge for a lane are straight vertical segments at `Lane::col`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub lane: u32,
    pub from_col: u32,
    pub to_col: u32,
    pub bend: Bend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// Column of the commit (always its lane's column).
    pub col: u32,
    pub lane: u32,
    /// Connectors in the span between this row and the next.
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub rows: Vec<Row>,
    pub lanes: Vec<Lane>,
    /// Number of columns needed to draw everything.
    pub max_cols: u32,
}

/// Best ref seen on a lane so far.
struct Candidate {
    rank: u8,
    row: usize,
    name: String,
    /// Name used for coloring (remote prefix removed so `main` and `origin/main` match).
    color_key: String,
}

struct LaneBuilder {
    col: u32,
    first_row: usize,
    last_row: usize,
    forked: bool,
    joins: bool,
    /// Branch name parsed from the merge commit that started the lane.
    merge_hint: Option<String>,
    best: Option<Candidate>,
}

impl LaneBuilder {
    fn new(col: usize, row: usize, forked: bool, merge_hint: Option<String>) -> Self {
        Self {
            col: to_u32(col),
            first_row: row,
            last_row: row,
            forked,
            joins: false,
            merge_hint,
            best: None,
        }
    }

    fn consider(&mut self, reference: &RefLabel) {
        let rank = match reference.kind {
            RefKind::Local if reference.is_head => 0,
            RefKind::Local => 1,
            RefKind::Remote => 2,
            RefKind::Tag => 3,
        };
        let better = self.best.as_ref().is_none_or(|best| {
            (rank, reference.row, &reference.name) < (best.rank, best.row, &best.name)
        });
        if better {
            let color_key = match reference.kind {
                RefKind::Remote => naming::strip_remote(&reference.name).to_string(),
                _ => reference.name.clone(),
            };
            self.best = Some(Candidate {
                rank,
                row: reference.row,
                name: reference.name.clone(),
                color_key,
            });
        }
    }

    fn finish(self, id: usize) -> Lane {
        let (name, kind, color_key) = match (self.best, self.merge_hint) {
            (Some(best), _) => (Some(best.name), LaneKind::Branch, Some(best.color_key)),
            (None, Some(hint)) => (Some(hint.clone()), LaneKind::Merged, Some(hint)),
            (None, None) => (None, LaneKind::Anonymous, None),
        };
        Lane {
            id: to_u32(id),
            col: self.col,
            first_row: self.first_row,
            last_row: self.last_row,
            forked: self.forked,
            joins: self.joins,
            color: color_key.as_deref().map(naming::color_for),
            name,
            kind,
        }
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).expect("graph too large for u32 indices")
}

/// Lowest free column (the tip of a new branch goes there).
fn lowest_free(slots: &[Option<usize>]) -> usize {
    slots
        .iter()
        .position(Option::is_none)
        .unwrap_or(slots.len())
}

/// Free column closest to `from`, preferring the right side on ties.
fn nearest_free(slots: &[Option<usize>], from: usize) -> usize {
    let is_free = |col: usize| slots.get(col).is_none_or(Option::is_none);
    (1..)
        .find_map(|distance| {
            if is_free(from + distance) {
                Some(from + distance)
            } else if from >= distance && is_free(from - distance) {
                Some(from - distance)
            } else {
                None
            }
        })
        .expect("the right side is eventually free")
}

fn occupy(slots: &mut Vec<Option<usize>>, col: usize, lane: usize) {
    if col == slots.len() {
        slots.push(Some(lane));
    } else {
        slots[col] = Some(lane);
    }
}

/// Computes the layout. `commits` must be in topological order (see [`topological_order`]).
pub fn layout(commits: &[Commit], refs: &[RefLabel]) -> Layout {
    let mut refs_at_row: HashMap<usize, Vec<&RefLabel>> = HashMap::new();
    for reference in refs.iter().filter(|r| r.row < commits.len()) {
        refs_at_row
            .entry(reference.row)
            .or_default()
            .push(reference);
    }

    // Lane id occupying each column right after the current row.
    let mut slots: Vec<Option<usize>> = Vec::new();
    // Row index -> columns of the lanes heading to that commit.
    let mut heading_to: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut builders: Vec<LaneBuilder> = Vec::new();
    let mut rows: Vec<Row> = Vec::with_capacity(commits.len());

    for (row, commit) in commits.iter().enumerate() {
        debug_assert!(
            commit.parents.iter().all(|&p| p > row),
            "commits must be in topological order"
        );

        let mut converging = heading_to.remove(&row).unwrap_or_default();
        converging.sort_unstable();

        let (col, lane) = if let Some(&col) = converging.first() {
            (col, slots[col].expect("a heading lane occupies its column"))
        } else {
            // Nothing points at this commit: it is the tip of a new line.
            let col = lowest_free(&slots);
            let lane = builders.len();
            builders.push(LaneBuilder::new(col, row, false, None));
            occupy(&mut slots, col, lane);
            (col, lane)
        };

        // The other lanes heading here end at this commit, bending into its column.
        for &other_col in converging.iter().skip(1) {
            let ended = slots[other_col]
                .take()
                .expect("a heading lane occupies its column");
            builders[ended].last_row = row;
            builders[ended].joins = true;
            let previous = rows
                .last_mut()
                .expect("a converging lane implies a previous row");
            let ended_id = to_u32(ended);
            if let Some(edge) = previous.edges.iter_mut().find(|e| e.lane == ended_id) {
                edge.to_col = to_u32(col);
            } else {
                previous.edges.push(Edge {
                    lane: ended_id,
                    from_col: to_u32(other_col),
                    to_col: to_u32(col),
                    bend: Bend::End,
                });
            }
        }

        builders[lane].last_row = row;
        for reference in refs_at_row.get(&row).into_iter().flatten() {
            builders[lane].consider(reference);
        }

        let mut edges = Vec::new();
        match commit.parents.first() {
            Some(&first_parent) => heading_to.entry(first_parent).or_default().push(col),
            None => slots[col] = None,
        }
        for (index, &parent) in commit.parents.iter().enumerate().skip(1) {
            let new_col = nearest_free(&slots, col);
            let hint = if index == 1 {
                merge_branch_name(&commit.subject)
            } else {
                None
            };
            let new_lane = builders.len();
            builders.push(LaneBuilder::new(new_col, row, true, hint));
            occupy(&mut slots, new_col, new_lane);
            heading_to.entry(parent).or_default().push(new_col);
            edges.push(Edge {
                lane: to_u32(new_lane),
                from_col: to_u32(col),
                to_col: to_u32(new_col),
                bend: Bend::Start,
            });
        }

        rows.push(Row {
            col: to_u32(col),
            lane: to_u32(lane),
            edges,
        });
    }

    let max_cols = builders.iter().map(|b| b.col + 1).max().unwrap_or(0);
    let lanes = builders
        .into_iter()
        .enumerate()
        .map(|(id, b)| b.finish(id))
        .collect();
    Layout {
        rows,
        lanes,
        max_cols,
    }
}

#[cfg(test)]
pub(crate) mod tests;
