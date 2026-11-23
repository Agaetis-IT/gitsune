//! Read-only git access (backed by `gix`).

mod commit_detail;
mod graph;
mod graph_page;
mod refs;
mod repo;
mod search;
mod text_diff;

pub use commit_detail::{CommitDetail, FileDiff, commit_detail, file_diff};
pub use graph_page::{
    GraphCache, GraphMeta, GraphPage, build_graph_cache, graph_meta, graph_page, graph_search,
};
pub use refs::{RefInfo, list_refs};
pub use repo::{RepoInfo, open_repository};
pub use search::SearchResult;

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod tests;
