//! Paged access to the commit graph for the frontend.
//!
//! The whole graph is read and laid out once per repository ([`GraphCache`]); pages are
//! cheap slices of that layout, completed with commit details read on demand. Because
//! every page comes from the same layout, lanes and columns are consistent across pages.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use gix::ObjectId;
use gix::ThreadSafeRepository;
use serde::Serialize;

use super::graph::{CommitGraph, first_line, load_graph};
use super::refs::{RefInfo, list_refs};
use super::search::{SearchIndex, SearchResult, SearchStore, build_index, search};
use super::store::RepoStores;
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
    /// Row of every commit of the graph, to find a commit again after a rebuild.
    row_of: HashMap<ObjectId, usize>,
    /// Built the first time a search is made.
    search_index: OnceLock<SearchIndex>,
    /// Search entries of the repository, reused by the next builds.
    search_store: Arc<Mutex<SearchStore>>,
}

/// A visible ref and the row of the commit it points at.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRef {
    #[serde(flatten)]
    pub info: RefInfo,
    pub row: usize,
    /// Lane of that commit, to highlight the branch.
    pub lane: u32,
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
/// A build from scratch, for tests.
#[cfg(test)]
pub fn build_graph_cache(
    repo: &ThreadSafeRepository,
    hidden: &HashSet<String>,
) -> Result<GraphCache, String> {
    build_graph_cache_with(repo, hidden, &RepoStores::default())
}

/// Like [`build_graph_cache`], reusing what `stores` remembers from earlier builds: only the
/// commits that appeared since are read again. The result is the same as a build from scratch.
pub fn build_graph_cache_with(
    repo: &ThreadSafeRepository,
    hidden: &HashSet<String>,
    stores: &RepoStores,
) -> Result<GraphCache, String> {
    let refs: Vec<RefInfo> = list_refs(repo)?
        .into_iter()
        .filter(|r| !hidden.contains(&r.full_name))
        .collect();

    let started = Instant::now();
    let graph = {
        let mut commits = stores
            .commits
            .lock()
            .map_err(|_| "commit store poisoned".to_string())?;
        load_graph(repo, &refs, &mut commits)?
    };
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
            let lane = layout.rows[row].lane;
            graph_refs.push(GraphRef {
                info: reference,
                row,
                lane,
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
        row_of,
        search_index: OnceLock::new(),
        search_store: stores.search.clone(),
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

/// Finds commits of the graph matching `query` (see [`search`]). The search index is
/// built from the commits on the first call and kept for the following ones.
pub fn graph_search(
    repo: &ThreadSafeRepository,
    cache: &GraphCache,
    query: &str,
    limit: usize,
) -> SearchResult {
    let index = cache.search_index.get_or_init(|| {
        let mut store = cache.search_store.lock().unwrap_or_else(|e| e.into_inner());
        build_index(repo, &cache.graph.ids, &mut store)
    });
    search(index, query, limit)
}

/// Rows of the given commit ids in the graph (`None` for commits it does not contain).
pub fn locate_commits(cache: &GraphCache, ids: &[String]) -> Vec<Option<usize>> {
    ids.iter()
        .map(|id| {
            let id = ObjectId::from_hex(id.as_bytes()).ok()?;
            cache.row_of.get(&id).copied()
        })
        .collect()
}
