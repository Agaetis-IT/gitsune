//! What is remembered about a repository between two graph builds, so that a refresh
//! only has to read the commits that appeared since the last one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gix::ObjectId;

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
}
