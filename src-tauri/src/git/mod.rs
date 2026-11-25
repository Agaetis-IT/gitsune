//! Read-only git access (backed by `gix`).

mod activity;
mod cli;
mod commit_detail;
mod graph;
mod graph_page;
mod ops;
mod refs;
mod repo;
mod search;
mod status;
mod store;
mod text_diff;
mod watch;
mod worktree_watch;

pub use activity::ActivityEntry;
pub use cli::GitError;
pub use commit_detail::{CommitDetail, FileDiff, commit_detail, file_diff};
pub use graph_page::{
    GraphCache, GraphMeta, GraphPage, build_graph_cache_with, graph_meta, graph_page, graph_search,
    locate_commits,
};
pub use ops::{
    Area, CommitResult, DiscardResult, Identity, OpCtx, Snapshot, commit, discard_paths, identity,
    list_snapshots, restore_snapshot, set_identity, stage_hunk, stage_paths, undo_commit,
    unstage_hunk, unstage_paths, working_diff,
};
pub use refs::{RefInfo, list_refs};
pub use repo::{RepoInfo, open_repository};
pub use search::SearchResult;
pub use status::WorkingStatus;
pub use store::RepoStores;
pub use watch::{RepoWatcher, watch_refs};
pub use worktree_watch::watch_worktree;

#[cfg(test)]
mod commit_tests;
#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod ops_tests;
#[cfg(test)]
mod refresh_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod tests;
