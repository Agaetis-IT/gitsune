//! What is remembered about a repository between two graph builds, so that a refresh
//! only has to read the commits that appeared since the last one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use gix::ObjectId;

use super::activity::ActivityLog;
use super::cli::{ErrorKind, GitError, Runner};
use super::search::SearchStore;

/// The parts of a commit the graph needs.
#[derive(Debug, Clone)]
pub struct StoredCommit {
    pub parents: Vec<ObjectId>,
    /// Committer time, seconds since the Unix epoch.
    pub time: i64,
    /// First line of the message, only kept for merge commits (it names merged branches).
    pub subject: String,
}

/// Commits already read, by id. Commits never change, so entries never go stale.
#[derive(Default)]
pub struct CommitStore {
    pub commits: HashMap<ObjectId, StoredCommit>,
}

/// Everything kept for one open repository.
#[derive(Default)]
pub struct RepoStores {
    pub commits: Mutex<CommitStore>,
    pub search: Arc<Mutex<SearchStore>>,
    /// The git commands run for this repository.
    pub activity: ActivityLog,
    /// While set, every write is refused before git is started.
    read_only: AtomicBool,
    /// Writes of the app run one at a time.
    write_lock: Mutex<()>,
    /// Extra environment for every git command (tests).
    pub env: Vec<(String, String)>,
}

impl RepoStores {
    #[cfg(test)]
    pub fn with_env(env: Vec<(String, String)>) -> Self {
        Self {
            env,
            ..Self::default()
        }
    }

    pub fn set_read_only(&self, read_only: bool) {
        self.read_only.store(read_only, Ordering::SeqCst);
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only.load(Ordering::SeqCst)
    }

    pub fn runner(&self) -> Runner<'_> {
        Runner {
            log: &self.activity,
            env: &self.env,
        }
    }

    /// Waits for the previous write to finish and refuses if the repository is read-only.
    /// Hold the returned guard for the whole write.
    pub fn begin_write(&self) -> Result<MutexGuard<'_, ()>, GitError> {
        if self.is_read_only() {
            return Err(GitError::new(
                ErrorKind::ReadOnly,
                "This repository is in read-only mode.",
            ));
        }
        let guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        // The mode may have been switched on while waiting.
        if self.is_read_only() {
            return Err(GitError::new(
                ErrorKind::ReadOnly,
                "This repository is in read-only mode.",
            ));
        }
        Ok(guard)
    }
}
