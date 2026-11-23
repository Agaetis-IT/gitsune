//! Watches the refs of a repository and tells when they really changed.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gix::ThreadSafeRepository;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEventBatch, Debouncer, new_debouncer};

use super::refs::{RefInfo, list_refs};

/// Keeps watching for as long as it lives.
pub struct RepoWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
}

/// The refs of a repository and where HEAD points: what a graph depends on.
#[derive(PartialEq, Eq)]
struct RefsState {
    refs: Vec<RefInfo>,
    head: Option<String>,
}

fn refs_state(repo: &ThreadSafeRepository) -> Option<RefsState> {
    let head = repo
        .to_thread_local()
        .head_id()
        .ok()
        .map(|id| id.to_string());
    Some(RefsState {
        refs: list_refs(repo).ok()?,
        head,
    })
}

/// Paths whose change can move a ref: `HEAD`, `packed-refs`, and anything under `refs/`
/// (lock files excluded). Everything else in the git directory (objects, index, logs...) is noise.
struct RelevantPaths {
    /// (git dir, common dir) as given by the repository and as canonicalized: depending on the
    /// platform, events carry either form (on macOS `/var` is really `/private/var`).
    dirs: Vec<(PathBuf, PathBuf)>,
}

impl RelevantPaths {
    fn new(repo: &ThreadSafeRepository) -> Self {
        let repo = repo.to_thread_local();
        let (git_dir, common_dir) = (
            repo.git_dir().to_path_buf(),
            repo.common_dir().to_path_buf(),
        );
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut dirs = vec![
            (canonical(&git_dir), canonical(&common_dir)),
            (git_dir, common_dir),
        ];
        dirs.dedup();
        Self { dirs }
    }

    fn matches(&self, path: &Path) -> bool {
        if path.extension().is_some_and(|ext| ext == "lock") {
            return false;
        }
        self.dirs.iter().any(|(git_dir, common_dir)| {
            path == git_dir.join("HEAD")
                || path == common_dir.join("packed-refs")
                || path.starts_with(common_dir.join("refs"))
        })
    }
}

/// Calls `on_change` (from another thread) after the refs or HEAD of the repository changed.
///
/// Changes are grouped over `debounce`, and a group that leaves the refs exactly as they were
/// (a `git gc` packing refs, a fetch with nothing new) does not call `on_change`.
pub fn watch_refs(
    repo: &ThreadSafeRepository,
    debounce: Duration,
    on_change: impl Fn() + Send + 'static,
) -> Result<RepoWatcher, String> {
    let relevant = RelevantPaths::new(repo);
    let known = Arc::new(Mutex::new(refs_state(repo)));
    let repo_for_handler = repo.clone();

    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        let touched = match result {
            Ok(DebouncedEventBatch::Events(events)) => {
                events.iter().any(|event| relevant.matches(&event.path))
            }
            // Events were missed (or something we do not know): look at the refs to be sure.
            Ok(_) => true,
            Err(_) => return,
        };
        if !touched {
            return;
        }
        let Some(now) = refs_state(&repo_for_handler) else {
            return;
        };
        let mut known = known.lock().unwrap_or_else(|e| e.into_inner());
        if known.as_ref() != Some(&now) {
            *known = Some(now);
            drop(known);
            on_change();
        }
    })
    .map_err(|e| format!("cannot watch the repository: {e}"))?;

    let local = repo.to_thread_local();
    let watcher = debouncer.watcher();
    let watch = |watcher: &mut dyn notify::Watcher, path: &Path, mode: RecursiveMode| {
        watcher
            .watch(path, mode)
            .map_err(|e| format!("cannot watch {}: {e}", path.display()))
    };
    // HEAD lives in the git dir; refs and packed-refs in the common dir (the same, except for worktrees).
    watch(watcher, local.git_dir(), RecursiveMode::NonRecursive)?;
    if local.common_dir() != local.git_dir() {
        watch(watcher, local.common_dir(), RecursiveMode::NonRecursive)?;
    }
    let refs_dir = local.common_dir().join("refs");
    if refs_dir.is_dir() {
        watch(watcher, &refs_dir, RecursiveMode::Recursive)?;
    }
    Ok(RepoWatcher {
        _debouncer: debouncer,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;
    use std::time::Instant;

    use tempfile::TempDir;

    use super::*;
    use crate::git::open_repository;
    use crate::git::tests::{commit, git};

    const DEBOUNCE: Duration = Duration::from_millis(100);

    fn watched(dir: &Path) -> (RepoWatcher, Arc<AtomicUsize>) {
        let (repo, _) = open_repository(dir).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let watcher = watch_refs(&repo, DEBOUNCE, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        // Let the file system watcher settle before the test changes anything.
        sleep(Duration::from_millis(300));
        (watcher, calls)
    }

    fn repo_with_commit() -> TempDir {
        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        commit(tmp.path(), "first");
        tmp
    }

    /// Waits until `calls` reaches `at_least`, up to 5 seconds.
    fn wait_for(calls: &AtomicUsize, at_least: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if calls.load(Ordering::SeqCst) >= at_least {
                return true;
            }
            sleep(Duration::from_millis(50));
        }
        false
    }

    /// Time to be sure nothing is coming (debounce plus file system latency).
    fn settle() {
        sleep(Duration::from_millis(1500));
    }

    #[test]
    fn a_new_commit_is_reported() {
        let tmp = repo_with_commit();
        let (_watcher, calls) = watched(tmp.path());
        commit(tmp.path(), "second");
        assert!(wait_for(&calls, 1), "no change reported after a commit");
    }

    #[test]
    fn a_new_branch_and_a_checkout_are_reported() {
        let tmp = repo_with_commit();
        let (_watcher, calls) = watched(tmp.path());

        git(tmp.path(), &["branch", "feature/x"]);
        assert!(wait_for(&calls, 1), "no change reported for a new branch");
        settle();
        let after_branch = calls.load(Ordering::SeqCst);

        git(tmp.path(), &["checkout", "-q", "feature/x"]);
        assert!(
            wait_for(&calls, after_branch + 1),
            "no change reported for a checkout"
        );
    }

    #[test]
    fn a_new_tag_and_a_deleted_branch_are_reported() {
        let tmp = repo_with_commit();
        git(tmp.path(), &["branch", "old"]);
        let (_watcher, calls) = watched(tmp.path());

        git(tmp.path(), &["tag", "v1"]);
        assert!(wait_for(&calls, 1), "no change reported for a tag");
        settle();
        let after_tag = calls.load(Ordering::SeqCst);
        git(tmp.path(), &["branch", "-q", "-D", "old"]);
        assert!(
            wait_for(&calls, after_tag + 1),
            "no change reported for a deleted branch"
        );
    }

    #[test]
    fn changes_that_do_not_move_a_ref_are_ignored() {
        let tmp = repo_with_commit();
        let (_watcher, calls) = watched(tmp.path());

        // Staging touches the index, not the refs.
        std::fs::write(tmp.path().join("notes.txt"), "invented notes\n").unwrap();
        git(tmp.path(), &["add", "notes.txt"]);
        settle();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the index is not a ref");

        // Packing refs rewrites files under refs/ but leaves every ref where it was.
        git(tmp.path(), &["pack-refs", "--all"]);
        settle();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "packing refs changes nothing"
        );
    }

    #[test]
    fn a_burst_of_changes_is_grouped() {
        let tmp = repo_with_commit();
        let (_watcher, calls) = watched(tmp.path());
        for i in 0..5 {
            git(tmp.path(), &["branch", &format!("burst-{i}")]);
        }
        assert!(wait_for(&calls, 1));
        settle();
        assert!(
            calls.load(Ordering::SeqCst) < 5,
            "five branches should not give five reports"
        );
    }

    #[test]
    fn nothing_is_reported_after_the_watcher_is_dropped() {
        let tmp = repo_with_commit();
        let (watcher, calls) = watched(tmp.path());
        drop(watcher);
        commit(tmp.path(), "second");
        settle();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
