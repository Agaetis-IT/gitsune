//! Search over the commits of a graph: message, author and commit id prefix.

use gix::{ObjectId, ThreadSafeRepository};
use serde::Serialize;

/// Most matches returned in one search.
pub const MAX_RESULTS: usize = 2000;
/// Bytes of each commit message kept in the index.
const MAX_MESSAGE_BYTES: usize = 2000;

/// Lowercased searchable text of every commit, indexed by row.
pub struct SearchIndex {
    ids: Vec<String>,
    authors: Vec<String>,
    messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Matching rows, from the newest commit down, at most the requested limit.
    pub rows: Vec<usize>,
    /// Number of matching commits (can exceed `rows.len()`).
    pub total: usize,
}

/// Reads the author and message of every commit. Commits that cannot be read are never matched.
pub fn build_index(repo: &ThreadSafeRepository, ids: &[ObjectId]) -> SearchIndex {
    let repo = repo.to_thread_local();
    let mut index = SearchIndex {
        ids: Vec::with_capacity(ids.len()),
        authors: Vec::with_capacity(ids.len()),
        messages: Vec::with_capacity(ids.len()),
    };
    for id in ids {
        index.ids.push(id.to_string());
        match repo.find_commit(*id) {
            Ok(commit) => {
                let author = commit
                    .author()
                    .map(|a| a.name.to_string())
                    .unwrap_or_default();
                let raw = commit.message_raw_sloppy();
                let message = String::from_utf8_lossy(&raw[..raw.len().min(MAX_MESSAGE_BYTES)]);
                index.authors.push(author.to_lowercase());
                index.messages.push(message.to_lowercase());
            }
            Err(_) => {
                index.authors.push(String::new());
                index.messages.push(String::new());
            }
        }
    }
    index
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
    for row in 0..index.ids.len() {
        let matches = terms.iter().all(|term| {
            index.messages[row].contains(term.as_str())
                || index.authors[row].contains(term.as_str())
                || index.ids[row].starts_with(term.as_str())
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
