//! What is remembered about a repository between two graph builds, so that a refresh
//! only has to read the commits that appeared since the last one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

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
    /// `git status` runs one at a time: a burst of changes never starts several of them.
    status_lock: Mutex<()>,
    status_running: AtomicUsize,
    /// The most `git status` runs seen at once (always 1 unless the lock is broken).
    pub status_peak: AtomicUsize,
    /// How long the last `git status` took, in milliseconds.
    pub last_status_ms: AtomicU64,
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

    /// Runs `read` as the only `git status` of this repository at that moment.
    pub fn exclusive_status<T>(&self, read: impl FnOnce() -> T) -> T {
        let _only = self.status_lock.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.status_running.fetch_add(1, Ordering::SeqCst) + 1;
        self.status_peak.fetch_max(now, Ordering::SeqCst);
        let started = Instant::now();
        let result = read();
        self.last_status_ms
            .store(started.elapsed().as_millis() as u64, Ordering::SeqCst);
        self.status_running.fetch_sub(1, Ordering::SeqCst);
        result
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
