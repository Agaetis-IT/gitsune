//! Paged access to the commit graph for the frontend.
//!
//! The whole graph is read and laid out once per repository ([`GraphCache`]); pages are
//! cheap slices of that layout, completed with commit details read on demand. Because
//! every page comes from the same layout, lanes and columns are consistent across pages.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gix::ObjectId;
use gix::ThreadSafeRepository;
use serde::Serialize;

use super::graph::{CommitGraph, first_line, load_graph};
use super::refs::{RefInfo, list_refs};
use crate::layout::{self, Edge, Lane, Layout};

/// Largest page the backend will return, whatever the caller asks.
pub const MAX_PAGE_SIZE: usize = 1000;

/// Graph and layout of one repository, kept in memory between page requests.
pub struct GraphCache {
    graph: CommitGraph,
    layout: Layout,
    refs_by_row: HashMap<usize, Vec<RefInfo>>,
    /// Visible refs with the row they point at.
    refs: Vec<GraphRef>,
    load_ms: u128,
    layout_ms: u128,
}

/// A visible ref and the row of the commit it points at.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRef {
    #[serde(flatten)]
    pub info: RefInfo,
    pub row: usize,
}

/// What the frontend needs once, before asking for pages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMeta {
    /// Number of rows (commits) in the graph.
    pub total: usize,
    pub max_cols: u32,
    pub lanes: Vec<Lane>,
    /// Refs that seed the graph (hidden refs are left out).
    pub refs: Vec<GraphRef>,
    pub load_ms: u128,
    pub layout_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRow {
    pub row: usize,
    pub id: String,
    pub parents: Vec<String>,
    pub summary: String,
    pub author: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub col: u32,
    pub lane: u32,
    /// Non-vertical connectors between this row and the next one.
    pub edges: Vec<Edge>,
    /// Refs pointing at this commit.
    pub refs: Vec<RefInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPage {
    pub start: usize,
    pub rows: Vec<GraphRow>,
    /// Cursor for the following page, `None` at the end of the graph.
    pub next_cursor: Option<usize>,
    pub total: usize,
}

/// Reads and lays out the graph of everything reachable from the refs that are not hidden.
///
/// `hidden` holds full ref names (`refs/heads/main`). Commits only reachable from hidden refs
/// are left out of the graph.
pub fn build_graph_cache(
    repo: &ThreadSafeRepository,
    hidden: &HashSet<String>,
) -> Result<GraphCache, String> {
    let refs: Vec<RefInfo> = list_refs(repo)?
        .into_iter()
        .filter(|r| !hidden.contains(&r.full_name))
        .collect();

    let started = Instant::now();
    let graph = load_graph(repo, &refs)?;
    let load_ms = started.elapsed().as_millis();

    let started = Instant::now();
    let layout = layout::layout(&graph.commits, &graph.refs);
    let layout_ms = started.elapsed().as_millis();

    let row_of: HashMap<ObjectId, usize> = graph
        .ids
        .iter()
        .enumerate()
        .map(|(row, id)| (*id, row))
        .collect();
    let mut refs_by_row: HashMap<usize, Vec<RefInfo>> = HashMap::new();
    let mut graph_refs = Vec::with_capacity(refs.len());
    for reference in refs {
        let Ok(id) = ObjectId::from_hex(reference.target.as_bytes()) else {
            continue;
        };
        if let Some(&row) = row_of.get(&id) {
            refs_by_row.entry(row).or_default().push(reference.clone());
            graph_refs.push(GraphRef {
                info: reference,
                row,
            });
        }
    }

    Ok(GraphCache {
        graph,
        layout,
        refs_by_row,
        refs: graph_refs,
        load_ms,
        layout_ms,
    })
}

pub fn graph_meta(cache: &GraphCache) -> GraphMeta {
    GraphMeta {
        total: cache.graph.ids.len(),
        max_cols: cache.layout.max_cols,
        lanes: cache.layout.lanes.clone(),
        refs: cache.refs.clone(),
        load_ms: cache.load_ms,
        layout_ms: cache.layout_ms,
    }
}

/// Returns up to `limit` rows starting at row `start` (the cursor).
pub fn graph_page(
    repo: &ThreadSafeRepository,
    cache: &GraphCache,
    start: usize,
    limit: usize,
) -> Result<GraphPage, String> {
    let repo = repo.to_thread_local();
    let total = cache.graph.ids.len();
    let start = start.min(total);
    let end = (start + limit.clamp(1, MAX_PAGE_SIZE)).min(total);

    let mut rows = Vec::with_capacity(end - start);
    for row in start..end {
        let id = cache.graph.ids[row];
        let commit = repo
            .find_commit(id)
            .map_err(|e| format!("cannot read commit {id}: {e}"))?;
        let author = commit
            .author()
            .map(|a| a.name.to_string())
            .unwrap_or_default();
        let time = commit.time().map(|t| t.seconds).unwrap_or_default();
        let placed = &cache.layout.rows[row];
        rows.push(GraphRow {
            row,
            id: id.to_string(),
            parents: cache.graph.commits[row]
                .parents
                .iter()
                .map(|&parent| cache.graph.ids[parent].to_string())
                .collect(),
            summary: first_line(commit.message_raw_sloppy()),
            author,
            time,
            col: placed.col,
            lane: placed.lane,
            edges: placed.edges.clone(),
            refs: cache.refs_by_row.get(&row).cloned().unwrap_or_default(),
        });
    }

    Ok(GraphPage {
        start,
        rows,
        next_cursor: (end < total).then_some(end),
        total,
    })
}
