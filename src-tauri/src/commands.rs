use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gix::ThreadSafeRepository;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::git::{
    self, ActivityEntry, Area, CheckoutResult, CheckoutTarget, CommitDetail, CommitResult,
    DeletedBranch, DeletedTag, DirtyStrategy, DiscardResult, FileDiff, GitError, GraphCache,
    GraphMeta, GraphPage, RefInfo, RepoInfo, RepoStores, RepoWatcher, SearchResult, Snapshot,
    WorkingStatus,
};

struct OpenRepo {
    repo: ThreadSafeRepository,
    /// Layout of the whole graph, built on first use.
    graph: Option<Arc<GraphCache>>,
    /// Commits already read, so a rebuild only reads the new ones.
    stores: Arc<RepoStores>,
    /// Report changes of the refs and of the working tree for as long as the repository is open.
    _watchers: Vec<RepoWatcher>,
    /// Root of the working tree (`None` for a bare repository).
    workdir: Option<PathBuf>,
}

/// Payload of the `repo-changed` event: the refs of a repository moved.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoChanged {
    repo_id: String,
}

/// How long changes are grouped before `repo-changed` is sent.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

/// Opened repositories, keyed by repository id (its root path).
#[derive(Default)]
pub struct RepoRegistry {
    repos: Mutex<HashMap<String, OpenRepo>>,
}

const POISONED: &str = "repository registry poisoned";

impl RepoRegistry {
    /// Registers a repository with its watcher. Opening one that is already open keeps its
    /// cached graph and its watcher.
    fn insert(
        &self,
        id: String,
        repo: ThreadSafeRepository,
        watchers: Vec<RepoWatcher>,
        workdir: Option<PathBuf>,
    ) -> Result<(), String> {
        let mut repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        repos.entry(id).or_insert(OpenRepo {
            repo,
            graph: None,
            stores: Arc::new(RepoStores::default()),
            _watchers: watchers,
            workdir,
        });
        Ok(())
    }

    /// The working tree of a repository and what is kept for it, for the operations that
    /// read or write the working tree.
    fn context(&self, id: &str) -> Result<(PathBuf, Arc<RepoStores>), GitError> {
        let repos = self.repos.lock().map_err(|_| GitError::other(POISONED))?;
        let open = repos
            .get(id)
            .ok_or_else(|| GitError::other(format!("repository not open: {id}")))?;
        let workdir = open
            .workdir
            .clone()
            .ok_or_else(|| GitError::other("This repository has no working tree."))?;
        Ok((workdir, open.stores.clone()))
    }

    fn stores(&self, id: &str) -> Result<Arc<RepoStores>, GitError> {
        let repos = self.repos.lock().map_err(|_| GitError::other(POISONED))?;
        repos
            .get(id)
            .map(|open| open.stores.clone())
            .ok_or_else(|| GitError::other(format!("repository not open: {id}")))
    }

    fn is_open(&self, id: &str) -> bool {
        self.repos.lock().is_ok_and(|repos| repos.contains_key(id))
    }

    fn get(&self, id: &str) -> Result<ThreadSafeRepository, String> {
        let repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        repos
            .get(id)
            .map(|open| open.repo.clone())
            .ok_or_else(|| format!("repository not open: {id}"))
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        self.repos
            .lock()
            .map_err(|_| POISONED.to_string())?
            .remove(id);
        Ok(())
    }

    /// Returns the repository and its graph.
    ///
    /// With `rebuild_hiding` the graph is built again without the given hidden refs;
    /// otherwise the cached one is used (built with no hidden refs if there is none yet).
    /// The graph is built outside the lock so other repositories stay usable meanwhile.
    fn graph(
        &self,
        id: &str,
        rebuild_hiding: Option<&HashSet<String>>,
    ) -> Result<(ThreadSafeRepository, Arc<GraphCache>), String> {
        let (repo, cached, stores) = {
            let repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
            let open = repos
                .get(id)
                .ok_or_else(|| format!("repository not open: {id}"))?;
            (open.repo.clone(), open.graph.clone(), open.stores.clone())
        };
        if let (None, Some(graph)) = (rebuild_hiding, &cached) {
            return Ok((repo, graph.clone()));
        }
        let none = HashSet::new();
        let graph = Arc::new(git::build_graph_cache_with(
            &repo,
            rebuild_hiding.unwrap_or(&none),
            &stores,
        )?);
        let mut repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        if let Some(open) = repos.get_mut(id) {
            open.graph = Some(graph.clone());
        }
        Ok((repo, graph))
    }
}

#[tauri::command]
pub fn open_repo(
    path: String,
    app: AppHandle,
    registry: State<RepoRegistry>,
) -> Result<RepoInfo, String> {
    let (repo, info) = git::open_repository(&PathBuf::from(path))?;
    let already_open = registry.is_open(&info.id);
    let mut watchers = Vec::new();
    if !already_open {
        // Without a watcher the repository still works, it just does not refresh by itself.
        let announce = |event: &'static str, app: AppHandle, repo_id: String| {
            move || {
                let _ = app.emit(
                    event,
                    RepoChanged {
                        repo_id: repo_id.clone(),
                    },
                );
            }
        };
        let refs = git::watch_refs(
            &repo,
            REFRESH_DEBOUNCE,
            announce("repo-changed", app.clone(), info.id.clone()),
        );
        watchers.extend(refs.map_err(|error| eprintln!("{error}")).ok());
        if !info.is_bare {
            let tree = git::watch_worktree(
                &repo,
                REFRESH_DEBOUNCE,
                announce("worktree-changed", app.clone(), info.id.clone()),
            );
            watchers.extend(tree.map_err(|error| eprintln!("{error}")).ok());
        }
    }
    let workdir = (!info.is_bare).then(|| PathBuf::from(&info.path));
    registry.insert(info.id.clone(), repo, watchers, workdir)?;
    if !already_open && let Ok(stores) = registry.stores(&info.id) {
        let (handle, repo_id) = (app, info.id.clone());
        stores.activity.set_notifier(move || {
            let _ = handle.emit(
                "activity-changed",
                RepoChanged {
                    repo_id: repo_id.clone(),
                },
            );
        });
    }
    Ok(info)
}

#[tauri::command]
pub fn close_repo(repo_id: String, registry: State<RepoRegistry>) -> Result<(), String> {
    registry.remove(&repo_id)
}

#[tauri::command]
pub fn list_refs(repo_id: String, registry: State<RepoRegistry>) -> Result<Vec<RefInfo>, String> {
    git::list_refs(&registry.get(&repo_id)?)
}

/// Reads and lays out the graph (replacing any previous layout) and returns what the
/// frontend needs before requesting pages: totals, lanes and refs.
///
/// `hidden_refs` are full ref names whose commits are left out of the graph.
#[tauri::command]
pub async fn get_graph_meta(
    repo_id: String,
    hidden_refs: Vec<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<GraphMeta, String> {
    let hidden: HashSet<String> = hidden_refs.into_iter().collect();
    let (_, graph) = registry.graph(&repo_id, Some(&hidden))?;
    Ok(git::graph_meta(&graph))
}

/// Returns `limit` rows (at most 1000) starting at row `cursor`.
#[tauri::command]
pub async fn get_graph_page(
    repo_id: String,
    cursor: usize,
    limit: usize,
    registry: State<'_, RepoRegistry>,
) -> Result<GraphPage, String> {
    let (repo, graph) = registry.graph(&repo_id, None)?;
    git::graph_page(&repo, &graph, cursor, limit)
}

/// Message and changed files of a commit (against its first parent).
#[tauri::command]
pub async fn get_commit_detail(
    repo_id: String,
    id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<CommitDetail, String> {
    git::commit_detail(&registry.get(&repo_id)?, &id)
}

/// Diff of one file of a commit. Large files are only diffed when `force` is set.
#[tauri::command]
pub async fn get_file_diff(
    repo_id: String,
    id: String,
    path: String,
    old_path: Option<String>,
    force: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<FileDiff, String> {
    git::file_diff(
        &registry.get(&repo_id)?,
        &id,
        &path,
        old_path.as_deref(),
        force,
    )
}

/// Commits of the current graph whose message or author contains every word of `query`,
/// or whose id starts with it. Rows are returned newest first, at most `limit` (2000).
#[tauri::command]
pub async fn search_commits(
    repo_id: String,
    query: String,
    limit: usize,
    registry: State<'_, RepoRegistry>,
) -> Result<SearchResult, String> {
    let (repo, graph) = registry.graph(&repo_id, None)?;
    Ok(git::graph_search(&repo, &graph, &query, limit))
}

/// Rows of the given commit ids in the current graph (`null` for commits it does not contain).
/// Used after a refresh to find where the selected commit and the top of the view went.
#[tauri::command]
pub async fn locate_commits(
    repo_id: String,
    ids: Vec<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<Vec<Option<usize>>, String> {
    let (_, graph) = registry.graph(&repo_id, None)?;
    Ok(git::locate_commits(&graph, &ids))
}

// ---------- working tree: status, staging, commits ----------

/// Runs an operation of the working tree of an open repository.
fn with_ctx<T>(
    registry: &RepoRegistry,
    repo_id: &str,
    operation: impl FnOnce(&git::OpCtx) -> Result<T, GitError>,
) -> Result<T, GitError> {
    let (workdir, stores) = registry.context(repo_id)?;
    operation(&git::OpCtx {
        workdir: &workdir,
        stores: &stores,
    })
}

/// The changes in the index and in the working tree.
#[tauri::command]
pub async fn get_status(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| ctx.status())
}

/// Diff of a file of the working tree: the staged side (index against the last commit) or
/// the unstaged side (working tree against the index).
#[tauri::command]
pub async fn get_working_diff(
    repo_id: String,
    path: String,
    old_path: Option<String>,
    area: Area,
    force: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<FileDiff, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::working_diff(ctx, &path, old_path.as_deref(), area, force)
    })
}

#[tauri::command]
pub async fn stage_paths(
    repo_id: String,
    paths: Vec<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::stage_paths(ctx, &paths))
}

#[tauri::command]
pub async fn unstage_paths(
    repo_id: String,
    paths: Vec<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::unstage_paths(ctx, &paths))
}

#[tauri::command]
pub async fn stage_hunk(
    repo_id: String,
    path: String,
    hunk_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stage_hunk(ctx, &path, &hunk_id)
    })
}

#[tauri::command]
pub async fn unstage_hunk(
    repo_id: String,
    path: String,
    hunk_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::unstage_hunk(ctx, &path, &hunk_id)
    })
}

/// Throws away the unstaged changes of files, after saving a snapshot that brings them back.
#[tauri::command]
pub async fn discard_paths(
    repo_id: String,
    paths: Vec<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<DiscardResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::discard_paths(ctx, &paths))
}

#[tauri::command]
pub async fn list_snapshots(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<Vec<Snapshot>, GitError> {
    with_ctx(&registry, &repo_id, git::list_snapshots)
}

#[tauri::command]
pub async fn restore_snapshot(
    repo_id: String,
    reference: String,
    registry: State<'_, RepoRegistry>,
) -> Result<DiscardResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::restore_snapshot(ctx, &reference)
    })
}

/// Commits the index, or amends the last commit.
#[tauri::command]
pub async fn commit_changes(
    repo_id: String,
    summary: String,
    body: String,
    amend: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<CommitResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::commit(ctx, &summary, &body, amend)
    })
}

/// Moves HEAD back to where it was before a commit made by `commit_changes`.
#[tauri::command]
pub async fn undo_commit(
    repo_id: String,
    before: Option<String>,
    after: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::undo_commit(ctx, before.as_deref(), &after)
    })
}

#[tauri::command]
pub async fn get_identity(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<git::Identity, GitError> {
    with_ctx(&registry, &repo_id, git::identity)
}

#[tauri::command]
pub async fn set_identity(
    repo_id: String,
    name: String,
    email: String,
    global: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<git::Identity, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::set_identity(ctx, &name, &email, global)
    })
}

// ---------- safety and transparency ----------

/// Turns the read-only mode of a repository on or off (every write is refused while on).
#[tauri::command]
pub fn set_read_only(
    repo_id: String,
    read_only: bool,
    registry: State<RepoRegistry>,
) -> Result<(), GitError> {
    registry.stores(&repo_id)?.set_read_only(read_only);
    Ok(())
}

/// The git commands the app ran for a repository, oldest first.
#[tauri::command]
pub fn get_activity(
    repo_id: String,
    registry: State<RepoRegistry>,
) -> Result<Vec<ActivityEntry>, GitError> {
    Ok(registry.stores(&repo_id)?.activity.list())
}

#[tauri::command]
pub fn clear_activity(repo_id: String, registry: State<RepoRegistry>) -> Result<(), GitError> {
    registry.stores(&repo_id)?.activity.clear();
    Ok(())
}

// ---------- branches, checkout and tags ----------

/// Creates a branch at `start` (HEAD by default), optionally switching to it.
#[tauri::command]
pub async fn create_branch(
    repo_id: String,
    name: String,
    start: Option<String>,
    checkout: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::create_branch(ctx, &name, start.as_deref(), checkout)
    })
}

/// Switches to a branch, a remote branch, a commit or a tag. When local changes are in the
/// way the error lists the files; ask again with `strategy` ("stash" or "discard").
#[tauri::command]
pub async fn checkout_target(
    repo_id: String,
    target: CheckoutTarget,
    strategy: Option<DirtyStrategy>,
    registry: State<'_, RepoRegistry>,
) -> Result<CheckoutResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::checkout(ctx, &target, strategy)
    })
}

#[tauri::command]
pub async fn rename_branch(
    repo_id: String,
    old_name: String,
    new_name: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::rename_branch(ctx, &old_name, &new_name)
    })
}

/// Deletes a local branch; a branch with unmerged commits needs `force`.
#[tauri::command]
pub async fn delete_branch(
    repo_id: String,
    name: String,
    force: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<DeletedBranch, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::delete_branch(ctx, &name, force)
    })
}

#[tauri::command]
pub async fn restore_branch(
    repo_id: String,
    name: String,
    tip: String,
    upstream: Option<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::restore_branch(ctx, &name, &tip, upstream.as_deref())
    })
}

#[tauri::command]
pub async fn set_upstream(
    repo_id: String,
    branch: String,
    upstream: Option<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::set_upstream(ctx, &branch, upstream.as_deref())
    })
}

/// Creates a tag at `target` (HEAD by default); annotated when a message is given.
#[tauri::command]
pub async fn create_tag(
    repo_id: String,
    name: String,
    target: Option<String>,
    message: Option<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::create_tag(ctx, &name, target.as_deref(), message.as_deref())
    })
}

#[tauri::command]
pub async fn delete_tag(
    repo_id: String,
    name: String,
    registry: State<'_, RepoRegistry>,
) -> Result<DeletedTag, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::delete_tag(ctx, &name))
}

#[tauri::command]
pub async fn restore_tag(
    repo_id: String,
    name: String,
    target: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::restore_tag(ctx, &name, &target)
    })
}
