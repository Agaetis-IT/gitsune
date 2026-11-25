use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use std::time::Instant;

use gix::ThreadSafeRepository;
use serde::Serialize;
use tauri::State;

use crate::git::{self, RefInfo, RepoInfo};

/// Opened repositories, keyed by repository id (its root path).
#[derive(Default)]
pub struct RepoRegistry {
    repos: Mutex<HashMap<String, ThreadSafeRepository>>,
}

impl RepoRegistry {
    fn insert(&self, id: String, repo: ThreadSafeRepository) -> Result<(), String> {
        self.repos
            .lock()
            .map_err(|_| "repository registry poisoned".to_string())?
            .insert(id, repo);
        Ok(())
    }

    fn get(&self, id: &str) -> Result<ThreadSafeRepository, String> {
        self.repos
            .lock()
            .map_err(|_| "repository registry poisoned".to_string())?
            .get(id)
            .cloned()
            .ok_or_else(|| format!("repository not open: {id}"))
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        self.repos
            .lock()
            .map_err(|_| "repository registry poisoned".to_string())?
            .remove(id);
        Ok(())
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

/// Size and timing of the layout of a whole repository (smoke test for the layout engine).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSummary {
    pub commits: usize,
    pub lanes: usize,
    pub named_lanes: usize,
    pub max_cols: u32,
    pub load_ms: u128,
    pub layout_ms: u128,
}

#[tauri::command]
pub fn layout_summary(
    repo_id: String,
    registry: State<RepoRegistry>,
) -> Result<LayoutSummary, String> {
    let repo = registry.get(&repo_id)?;
    let refs = git::list_refs(&repo)?;

    let started = Instant::now();
    let graph = git::load_graph(&repo, &refs)?;
    let load_ms = started.elapsed().as_millis();

    let started = Instant::now();
    let layout = crate::layout::layout(&graph.commits, &graph.refs);
    let layout_ms = started.elapsed().as_millis();

    Ok(LayoutSummary {
        commits: graph.commits.len(),
        lanes: layout.lanes.len(),
        named_lanes: layout.lanes.iter().filter(|l| l.name.is_some()).count(),
        max_cols: layout.max_cols,
        load_ms,
        layout_ms,
    })
}
