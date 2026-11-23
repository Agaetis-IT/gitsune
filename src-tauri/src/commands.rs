use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gix::ThreadSafeRepository;
use tauri::State;

use crate::git::{self, GraphCache, GraphMeta, GraphPage, RefInfo, RepoInfo};

struct OpenRepo {
    repo: ThreadSafeRepository,
    /// Layout of the whole graph, built on first use.
    graph: Option<Arc<GraphCache>>,
}

/// Opened repositories, keyed by repository id (its root path).
#[derive(Default)]
pub struct RepoRegistry {
    repos: Mutex<HashMap<String, OpenRepo>>,
}

const POISONED: &str = "repository registry poisoned";

impl RepoRegistry {
    fn insert(&self, id: String, repo: ThreadSafeRepository) -> Result<(), String> {
        let mut repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        repos.insert(id, OpenRepo { repo, graph: None });
        Ok(())
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

    /// Returns the repository and its graph, building (or rebuilding) the graph if needed.
    ///
    /// The graph is built outside the lock so other repositories stay usable meanwhile.
    fn graph(
        &self,
        id: &str,
        rebuild: bool,
    ) -> Result<(ThreadSafeRepository, Arc<GraphCache>), String> {
        let (repo, cached) = {
            let repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
            let open = repos
                .get(id)
                .ok_or_else(|| format!("repository not open: {id}"))?;
            (open.repo.clone(), open.graph.clone())
        };
        if let (false, Some(graph)) = (rebuild, cached) {
            return Ok((repo, graph));
        }
        let graph = Arc::new(git::build_graph_cache(&repo)?);
        let mut repos = self.repos.lock().map_err(|_| POISONED.to_string())?;
        if let Some(open) = repos.get_mut(id) {
            open.graph = Some(graph.clone());
        }
        Ok((repo, graph))
    }
}

#[tauri::command]
pub fn open_repo(path: String, registry: State<RepoRegistry>) -> Result<RepoInfo, String> {
    let (repo, info) = git::open_repository(&PathBuf::from(path))?;
    registry.insert(info.id.clone(), repo)?;
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

/// Reads and lays out the whole graph (replacing any previous layout) and returns
/// what the frontend needs before requesting pages: totals, lanes and refs.
#[tauri::command]
pub async fn get_graph_meta(
    repo_id: String,
    registry: State<'_, RepoRegistry>,
) -> Result<GraphMeta, String> {
    let (_, graph) = registry.graph(&repo_id, true)?;
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
    let (repo, graph) = registry.graph(&repo_id, false)?;
    git::graph_page(&repo, &graph, cursor, limit)
}
