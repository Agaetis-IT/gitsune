use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use gix::ThreadSafeRepository;
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
