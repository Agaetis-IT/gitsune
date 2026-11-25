//! Read-only git access (backed by `gix`).

mod activity;
mod branches;
mod cli;
mod commit_detail;
mod graph;
mod graph_page;
mod merge;
mod ops;
mod rebase;
mod refs;
mod remote;
mod repo;
mod rewrite;
mod search;
mod speed;
mod stash;
mod status;
mod store;
mod text_diff;
mod watch;
mod worktree_watch;

pub use activity::ActivityEntry;
pub use branches::{
    CheckoutResult, CheckoutTarget, DeletedBranch, DeletedTag, DirtyStrategy, checkout,
    create_branch, create_tag, delete_branch, delete_tag, rename_branch, restore_branch,
    restore_tag, set_upstream,
};
pub use cli::{GitError, LongIo, answer_prompt};
pub use commit_detail::{CommitDetail, FileDiff, commit_detail, file_diff};
pub use graph_page::{
    GraphCache, GraphMeta, GraphPage, build_graph_cache_with, graph_meta, graph_page, graph_search,
    locate_commits,
};
pub use merge::{
    ConflictFile, MergeInfo, MergeMode, MergeResult, Side, abort_operation, conflict_file,
    continue_operation, merge, merge_info, resolve_file, resolve_side, skip_operation,
};
pub use ops::{
    Area, CommitResult, DiscardResult, Identity, OpCtx, Snapshot, commit, discard_paths, identity,
    list_snapshots, restore_snapshot, set_identity, stage_hunk, stage_paths, undo_commit,
    unstage_hunk, unstage_paths, working_diff,
};
pub use rebase::{RebasePlan, RebaseResult, TodoItem, rebase, rebase_plan};
pub use refs::{RefInfo, list_refs};
pub use remote::{
    PullStrategy, PushOptions, RemoteResult, delete_remote_branch, fetch, list_remotes, pull, push,
};
pub use repo::{RepoInfo, open_repository};
pub use rewrite::{
    CommitOpResult, ResetMode, ResetResult, cherry_pick, reset, revert_commit, undo_replay,
    undo_reset,
};
pub use search::SearchResult;
pub use speed::{StatusHint, enable_untracked_cache, status_hint};
pub use stash::{
    DroppedStash, StashEntry, list_stashes, stash_apply, stash_drop, stash_pop, stash_push,
    stash_restore,
};
pub use status::WorkingStatus;
pub use store::RepoStores;
pub use watch::{RepoWatcher, watch_refs};
pub use worktree_watch::watch_worktree;

#[cfg(test)]
mod branch_tests;
#[cfg(test)]
mod commit_tests;
#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod ops_tests;
#[cfg(test)]
mod perf_tests;
#[cfg(test)]
mod rebase_tests;
#[cfg(test)]
mod refresh_tests;
#[cfg(test)]
mod remote_tests;
#[cfg(test)]
mod rewrite_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod tests;
