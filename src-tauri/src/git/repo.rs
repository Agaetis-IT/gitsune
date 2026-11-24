use std::path::Path;

use gix::ThreadSafeRepository;
use serde::Serialize;

/// Summary of an opened repository, sent to the frontend.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub id: String,
    pub path: String,
    pub name: String,
    /// Short name of the checked-out branch, `None` when HEAD is detached or unborn.
    pub head_branch: Option<String>,
    pub is_bare: bool,
    /// A linked working tree (made by `git worktree add`), not the main one.
    pub is_linked: bool,
}

/// Opens the repository containing `path` and returns it with its summary.
pub fn open_repository(path: &Path) -> Result<(ThreadSafeRepository, RepoInfo), String> {
    let repo = ThreadSafeRepository::open(path)
        .map_err(|e| format!("cannot open repository at {}: {e}", path.display()))?;
    let local = repo.to_thread_local();

    let root = local
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| local.git_dir().to_path_buf());
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let head_branch = local
        .head_name()
        .map_err(|e| format!("cannot read HEAD: {e}"))?
        .map(|full| full.shorten().to_string());

    let info = RepoInfo {
        id: root.to_string_lossy().into_owned(),
        path: root.to_string_lossy().into_owned(),
        name,
        head_branch,
        is_bare: local.is_bare(),
        is_linked: local.git_dir() != local.common_dir(),
    };
    Ok((repo, info))
}
