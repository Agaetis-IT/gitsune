//! Search over the commits of a graph: message, author and commit id prefix.

use std::collections::HashMap;
use std::sync::Arc;

use gix::{ObjectId, ThreadSafeRepository};
use serde::Serialize;

/// Most matches returned in one search.
pub const MAX_RESULTS: usize = 2000;
/// Bytes of each commit message kept in the index.
const MAX_MESSAGE_BYTES: usize = 2000;

/// Lowercased searchable text of one commit.
pub struct SearchEntry {
    id: String,
    author: String,
    message: String,
}

/// Entries already built, by commit id, kept between graph builds.
pub type SearchStore = HashMap<ObjectId, Arc<SearchEntry>>;

/// The searchable text of every commit of a graph, indexed by row.
pub struct SearchIndex {
    entries: Vec<Arc<SearchEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Matching rows, from the newest commit down, at most the requested limit.
    pub rows: Vec<usize>,
    /// Number of matching commits (can exceed `rows.len()`).
    pub total: usize,
}

/// Reads the author and message of the commits not yet in `store`. Commits that cannot be
/// read are never matched.
pub fn build_index(
    repo: &ThreadSafeRepository,
    ids: &[ObjectId],
    store: &mut SearchStore,
) -> SearchIndex {
    let repo = repo.to_thread_local();
    let mut entries = Vec::with_capacity(ids.len());
    for id in ids {
        let entry = store
            .entry(*id)
            .or_insert_with(|| Arc::new(read_entry(&repo, *id)))
            .clone();
        entries.push(entry);
    }
    SearchIndex { entries }
}

fn read_entry(repo: &gix::Repository, id: ObjectId) -> SearchEntry {
    let (author, message) = match repo.find_commit(id) {
        Ok(commit) => {
            let author = commit
                .author()
                .map(|a| a.name.to_string())
                .unwrap_or_default();
            let raw = commit.message_raw_sloppy();
            let message = String::from_utf8_lossy(&raw[..raw.len().min(MAX_MESSAGE_BYTES)]);
            (author.to_lowercase(), message.to_lowercase())
        }
        Err(_) => (String::new(), String::new()),
    };
    SearchEntry {
        id: id.to_string(),
        author,
        message,
    }
}

/// Commits matching every whitespace-separated term of `query`, case-insensitively.
/// A term matches a commit when it appears in its message or author, or starts its id.
pub fn search(index: &SearchIndex, query: &str, limit: usize) -> SearchResult {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return SearchResult {
            rows: Vec::new(),
            total: 0,
        };
    }
    let limit = limit.clamp(1, MAX_RESULTS);

    let mut rows = Vec::new();
    let mut total = 0;
    for (row, entry) in index.entries.iter().enumerate() {
        let matches = terms.iter().all(|term| {
            entry.message.contains(term.as_str())
                || entry.author.contains(term.as_str())
                || entry.id.starts_with(term.as_str())
        });
        if matches {
            total += 1;
            if rows.len() < limit {
                rows.push(row);
            }
        }
    }
    SearchResult { rows, total }
}
