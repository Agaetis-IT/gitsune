use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gix::ThreadSafeRepository;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::git::{
    self, ActivityEntry, Area, CheckoutResult, CheckoutTarget, CommitDetail, CommitOpResult,
    CommitResult, ConflictFile, DeletedBranch, DeletedTag, DirtyStrategy, DiscardResult,
    DroppedStash, FileDiff, GitError, GraphCache, GraphMeta, GraphPage, MergeInfo, MergeMode,
    MergeResult, PullStrategy, PushOptions, RebasePlan, RebaseResult, RefInfo, RemoteResult,
    RepoInfo, RepoStores, RepoWatcher, ResetMode, ResetResult, SearchResult, Side, Snapshot,
    StashEntry, StatusHint, TodoItem, WorkingStatus,
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

// ---------- fetch, pull and push ----------

/// A fetch, pull or push that is running: what is needed to cancel it and to answer its questions.
struct RunningOp {
    cancel: AtomicBool,
    /// Where the askpass helper of this operation leaves its questions.
    askpass_dir: PathBuf,
}

/// The long operations in progress, by operation id (chosen by the interface).
#[derive(Default)]
pub struct RemoteOps {
    running: Mutex<HashMap<String, Arc<RunningOp>>>,
}

/// Payload of `remote-progress`: a line git printed while the operation runs.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteProgress {
    repo_id: String,
    op_id: String,
    line: String,
}

/// Payload of `askpass-request`: git or ssh needs a secret or a confirmation.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AskpassRequest {
    repo_id: String,
    op_id: String,
    prompt_id: String,
    prompt: String,
}

/// Runs a long operation off the async runtime, streaming its progress and questions as events.
async fn run_remote(
    app: AppHandle,
    registry: &RepoRegistry,
    ops: &RemoteOps,
    repo_id: String,
    op_id: String,
    operation: impl FnOnce(&git::OpCtx, &git::LongIo) -> Result<RemoteResult, GitError> + Send + 'static,
) -> Result<RemoteResult, GitError> {
    let (workdir, stores) = registry.context(&repo_id)?;
    let safe_id: String = op_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if safe_id.is_empty() || safe_id != op_id {
        return Err(GitError::other("invalid operation id"));
    }
    let running = Arc::new(RunningOp {
        cancel: AtomicBool::new(false),
        askpass_dir: std::env::temp_dir().join(format!(
            "git-hydra-askpass-{}-{safe_id}",
            std::process::id()
        )),
    });
    ops.running
        .lock()
        .map_err(|_| GitError::other(POISONED))?
        .insert(op_id.clone(), running.clone());

    let job = {
        let running = running.clone();
        let (repo_id, op_id) = (repo_id.clone(), op_id.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let progress = {
                let (app, repo_id, op_id) = (app.clone(), repo_id.clone(), op_id.clone());
                move |line: &str| {
                    let _ = app.emit(
                        "remote-progress",
                        RemoteProgress {
                            repo_id: repo_id.clone(),
                            op_id: op_id.clone(),
                            line: line.to_string(),
                        },
                    );
                }
            };
            let prompt = {
                let (app, repo_id, op_id) = (app.clone(), repo_id.clone(), op_id.clone());
                move |prompt_id: &str, text: &str| {
                    let _ = app.emit(
                        "askpass-request",
                        AskpassRequest {
                            repo_id: repo_id.clone(),
                            op_id: op_id.clone(),
                            prompt_id: prompt_id.to_string(),
                            prompt: text.to_string(),
                        },
                    );
                }
            };
            let io = git::LongIo {
                cancel: &running.cancel,
                askpass_dir: &running.askpass_dir,
                on_progress: &progress,
                on_prompt: &prompt,
            };
            let result = operation(
                &git::OpCtx {
                    workdir: &workdir,
                    stores: &stores,
                },
                &io,
            );
            let _ = std::fs::remove_dir_all(&running.askpass_dir);
            result
        })
    };
    let result = job
        .await
        .map_err(|e| GitError::other(format!("the operation stopped unexpectedly: {e}")))?;
    if let Ok(mut running) = ops.running.lock() {
        running.remove(&op_id);
    }
    result
}

#[tauri::command]
pub async fn fetch_remote(
    repo_id: String,
    op_id: String,
    remote: Option<String>,
    prune: bool,
    app: AppHandle,
    registry: State<'_, RepoRegistry>,
    ops: State<'_, RemoteOps>,
) -> Result<RemoteResult, GitError> {
    run_remote(app, &registry, &ops, repo_id, op_id, move |ctx, io| {
        git::fetch(ctx, io, remote.as_deref(), prune)
    })
    .await
}

/// Fetches and integrates the upstream. Without a strategy git applies the user's
/// configuration; `divergedBranches` means it needs one.
#[tauri::command]
pub async fn pull_branch(
    repo_id: String,
    op_id: String,
    strategy: Option<PullStrategy>,
    app: AppHandle,
    registry: State<'_, RepoRegistry>,
    ops: State<'_, RemoteOps>,
) -> Result<RemoteResult, GitError> {
    run_remote(app, &registry, &ops, repo_id, op_id, move |ctx, io| {
        git::pull(ctx, io, strategy)
    })
    .await
}

#[tauri::command]
pub async fn push_branch(
    repo_id: String,
    op_id: String,
    options: PushOptions,
    app: AppHandle,
    registry: State<'_, RepoRegistry>,
    ops: State<'_, RemoteOps>,
) -> Result<RemoteResult, GitError> {
    run_remote(app, &registry, &ops, repo_id, op_id, move |ctx, io| {
        git::push(ctx, io, &options)
    })
    .await
}

#[tauri::command]
pub async fn delete_remote_branch(
    repo_id: String,
    op_id: String,
    remote: String,
    branch: String,
    app: AppHandle,
    registry: State<'_, RepoRegistry>,
    ops: State<'_, RemoteOps>,
) -> Result<RemoteResult, GitError> {
    run_remote(app, &registry, &ops, repo_id, op_id, move |ctx, io| {
        git::delete_remote_branch(ctx, io, &remote, &branch)
    })
    .await
}

/// The names of the remotes of the repository.
#[tauri::command]
pub async fn list_remotes(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<Vec<String>, GitError> {
    with_ctx(&registry, &repo_id, git::list_remotes)
}

/// Stops a running fetch, pull or push.
#[tauri::command]
pub async fn cancel_operation(op_id: String, ops: State<'_, RemoteOps>) -> Result<(), GitError> {
    let running = ops.running.lock().map_err(|_| GitError::other(POISONED))?;
    if let Some(op) = running.get(&op_id) {
        op.cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Gives git or ssh the answer to a question it asked (`None` refuses to answer).
#[tauri::command]
pub async fn answer_askpass(
    op_id: String,
    prompt_id: String,
    answer: Option<String>,
    ops: State<'_, RemoteOps>,
) -> Result<(), GitError> {
    let running = ops.running.lock().map_err(|_| GitError::other(POISONED))?;
    let op = running
        .get(&op_id)
        .ok_or_else(|| GitError::other("This operation is not running any more."))?;
    git::answer_prompt(&op.askpass_dir, &prompt_id, answer.as_deref())
}

// ---------- stash, cherry-pick, revert and reset ----------

#[tauri::command]
pub async fn list_stashes(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<Vec<StashEntry>, GitError> {
    with_ctx(&registry, &repo_id, git::list_stashes)
}

#[tauri::command]
pub async fn stash_push(
    repo_id: String,
    message: Option<String>,
    include_untracked: bool,
    keep_index: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stash_push(ctx, message.as_deref(), include_untracked, keep_index)
    })
}

/// `id` is the stash commit the interface saw at `reference`: a stale position is refused.
#[tauri::command]
pub async fn stash_apply(
    repo_id: String,
    reference: String,
    id: String,
    restore_index: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stash_apply(ctx, &reference, &id, restore_index)
    })
}

#[tauri::command]
pub async fn stash_pop(
    repo_id: String,
    reference: String,
    id: String,
    restore_index: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stash_pop(ctx, &reference, &id, restore_index)
    })
}

#[tauri::command]
pub async fn stash_drop(
    repo_id: String,
    reference: String,
    id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<DroppedStash, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stash_drop(ctx, &reference, &id)
    })
}

#[tauri::command]
pub async fn stash_restore(
    repo_id: String,
    id: String,
    message: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::stash_restore(ctx, &id, &message)
    })
}

#[tauri::command]
pub async fn cherry_pick(
    repo_id: String,
    commit: String,
    registry: State<'_, RepoRegistry>,
) -> Result<CommitOpResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::cherry_pick(ctx, &commit))
}

#[tauri::command]
pub async fn revert_commit(
    repo_id: String,
    commit: String,
    registry: State<'_, RepoRegistry>,
) -> Result<CommitOpResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::revert_commit(ctx, &commit))
}

/// Takes back a cherry-pick or a revert.
#[tauri::command]
pub async fn undo_replay(
    repo_id: String,
    before: String,
    after: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::undo_replay(ctx, &before, &after)
    })
}

#[tauri::command]
pub async fn reset_to(
    repo_id: String,
    target: String,
    mode: ResetMode,
    registry: State<'_, RepoRegistry>,
) -> Result<ResetResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::reset(ctx, &target, mode))
}

#[tauri::command]
pub async fn undo_reset(
    repo_id: String,
    before: String,
    after: String,
    snapshot: Option<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<DiscardResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::undo_reset(ctx, &before, &after, snapshot.as_deref())
    })
}

// ---------- merge and conflicts ----------

/// What merging `target` into the current branch would do.
#[tauri::command]
pub async fn merge_preview(
    repo_id: String,
    target: String,
    registry: State<'_, RepoRegistry>,
) -> Result<MergeInfo, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::merge_info(ctx, &target))
}

/// Merges a branch or a commit into the current branch. Conflicts are not an error: the
/// outcome says so and the merge is left in progress to be resolved.
#[tauri::command]
pub async fn merge_into_current(
    repo_id: String,
    target: String,
    mode: MergeMode,
    message: Option<String>,
    registry: State<'_, RepoRegistry>,
) -> Result<MergeResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::merge(ctx, &target, mode, message.as_deref())
    })
}

/// The three versions of a conflicted file and its content in the working tree.
#[tauri::command]
pub async fn get_conflict_file(
    repo_id: String,
    path: String,
    registry: State<'_, RepoRegistry>,
) -> Result<ConflictFile, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::conflict_file(ctx, &path))
}

/// Writes the resolved content of a conflicted file and marks it resolved.
#[tauri::command]
pub async fn resolve_conflict_file(
    repo_id: String,
    path: String,
    content: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::resolve_file(ctx, &path, &content)
    })
}

/// Resolves a conflicted file by taking "ours" or "theirs" as it is.
#[tauri::command]
pub async fn resolve_conflict_side(
    repo_id: String,
    path: String,
    side: Side,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::resolve_side(ctx, &path, side)
    })
}

/// Finishes the operation in progress (commits the merge...) once the conflicts are resolved.
#[tauri::command]
pub async fn continue_operation(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, git::continue_operation)
}

/// Cancels the operation in progress and puts the repository back as it was.
#[tauri::command]
pub async fn abort_operation(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, git::abort_operation)
}

// ---------- rebase ----------

/// The commits a rebase onto `onto` would replay, to show before asking.
#[tauri::command]
pub async fn rebase_preview(
    repo_id: String,
    onto: String,
    registry: State<'_, RepoRegistry>,
) -> Result<RebasePlan, GitError> {
    with_ctx(&registry, &repo_id, |ctx| git::rebase_plan(ctx, &onto))
}

/// Rebases the current branch onto `onto`. With `items` the list of commits is the one given
/// (interactive rebase). Conflicts and a stop for `edit` are not errors: the rebase stays in
/// progress and the outcome says so.
#[tauri::command]
pub async fn rebase_onto(
    repo_id: String,
    onto: String,
    items: Option<Vec<TodoItem>>,
    autostash: bool,
    registry: State<'_, RepoRegistry>,
) -> Result<RebaseResult, GitError> {
    with_ctx(&registry, &repo_id, |ctx| {
        git::rebase(ctx, &onto, items.as_deref(), autostash)
    })
}

/// Leaves out the commit a rebase or a cherry-pick is stuck on.
#[tauri::command]
pub async fn skip_operation(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<WorkingStatus, GitError> {
    with_ctx(&registry, &repo_id, git::skip_operation)
}

// ---------- keeping status fast ----------

/// How long `git status` takes, and whether the untracked cache would help.
#[tauri::command]
pub async fn get_status_hint(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<StatusHint, GitError> {
    with_ctx(&registry, &repo_id, git::status_hint)
}

/// Switches on git's untracked cache for this repository (a change to its configuration).
#[tauri::command]
pub async fn speed_up_status(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<StatusHint, GitError> {
    with_ctx(&registry, &repo_id, git::enable_untracked_cache)
}
