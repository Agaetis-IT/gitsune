use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gix::ThreadSafeRepository;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::git::{
    self, CommitDetail, FileDiff, GraphCache, GraphMeta, GraphPage, RefInfo, RepoInfo, RepoStores,
    RepoWatcher, SearchResult,
};

struct OpenRepo {
    repo: ThreadSafeRepository,
    /// Layout of the whole graph, built on first use.
    graph: Option<Arc<GraphCache>>,
    /// Commits already read, so a rebuild only reads the new ones.
    stores: Arc<RepoStores>,
    /// Reports changes of the refs for as long as the repository is open.
    _watcher: Option<RepoWatcher>,
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
        watcher: Option<RepoWatcher>,
    ) -> Result<(), String> {
        let mut repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        repos.entry(id).or_insert(OpenRepo {
            repo,
            graph: None,
            stores: Arc::new(RepoStores::default()),
            _watcher: watcher,
        });
        Ok(())
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
    let watcher = if registry.is_open(&info.id) {
        None
    } else {
        let repo_id = info.id.clone();
        // Without a watcher the repository still works, it just does not refresh by itself.
        git::watch_refs(&repo, REFRESH_DEBOUNCE, move || {
            let _ = app.emit(
                "repo-changed",
                RepoChanged {
                    repo_id: repo_id.clone(),
                },
            );
        })
        .map_err(|error| eprintln!("{error}"))
        .ok()
    };
    registry.insert(info.id.clone(), repo, watcher)?;
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
