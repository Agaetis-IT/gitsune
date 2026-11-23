//! Watches the working tree of a repository and tells when a file git cares about changed.
//!
//! Build tools can write thousands of files per second into ignored folders (`target/`,
//! `node_modules/`): events for ignored paths are dropped (asked to `git check-ignore`, one
//! call per burst), so only a real change makes the status worth reading again.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use gix::ThreadSafeRepository;
use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, DebouncedEventBatch, new_debouncer};

use super::watch::RepoWatcher;

/// Where the paths of the repository are, in the forms events may carry them (on macOS
/// `/var` is really `/private/var`).
struct Roots {
    workdirs: Vec<PathBuf>,
    git_dirs: Vec<PathBuf>,
}

impl Roots {
    fn new(workdir: &Path, git_dir: &Path) -> Self {
        let both = |path: &Path| {
            let mut forms = vec![path.canonicalize().unwrap_or_else(|_| path.to_path_buf())];
            if !forms.contains(&path.to_path_buf()) {
                forms.push(path.to_path_buf());
            }
            forms
        };
        Self {
            workdirs: both(workdir),
            git_dirs: both(git_dir),
        }
    }
}

enum Event {
    /// `HEAD` or the index changed: the status certainly did.
    Git,
    /// A file of the working tree, relative to its root: relevant unless git ignores it.
    Worktree(String),
    Noise,
}

fn classify(roots: &Roots, path: &Path) -> Event {
    for git_dir in &roots.git_dirs {
        if let Ok(inside) = path.strip_prefix(git_dir) {
            // The state of a merge, cherry-pick, revert or rebase in progress shows in the status.
            let relevant = [
                "index",
                "HEAD",
                "MERGE_HEAD",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
            ]
            .iter()
            .any(|name| inside == Path::new(name))
                || inside.starts_with("rebase-merge")
                || inside.starts_with("rebase-apply");
            return if relevant { Event::Git } else { Event::Noise };
        }
    }
    for workdir in &roots.workdirs {
        if let Ok(relative) = path.strip_prefix(workdir) {
            let text = relative.to_string_lossy();
            // Anything under .git (a linked worktree's, say) that is not ours.
            if text == ".git" || text.starts_with(".git/") {
                return Event::Noise;
            }
            return if text.is_empty() {
                Event::Noise
            } else {
                Event::Worktree(text.into_owned())
            };
        }
    }
    Event::Noise
}

/// The paths among `candidates` that git ignores.
fn ignored(workdir: &Path, candidates: &[String]) -> HashSet<String> {
    let Ok(mut child) = Command::new("git")
        .current_dir(workdir)
        .args(["check-ignore", "-z", "--stdin"])
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return HashSet::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let mut input = Vec::new();
        for path in candidates {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        // `check-ignore` answers while it reads: writing from another thread avoids a deadlock
        // when there are many paths.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
    let Ok(output) = child.wait_with_output() else {
        return HashSet::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Calls `on_change` (from another thread) after a file of the working tree that git does not
/// ignore changed, or after the index or `HEAD` changed. Changes are grouped over `debounce`.
pub fn watch_worktree(
    repo: &ThreadSafeRepository,
    debounce: Duration,
    on_change: impl Fn() + Send + 'static,
) -> Result<RepoWatcher, String> {
    let local = repo.to_thread_local();
    let workdir = local
        .workdir()
        .map(Path::to_path_buf)
        .ok_or_else(|| "a bare repository has no working tree".to_string())?;
    let roots = Roots::new(&workdir, local.git_dir());
    let check_dir = workdir.clone();

    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        let events = match result {
            Ok(DebouncedEventBatch::Events(events)) => events,
            // Events were missed (or something we do not know): the status may be stale.
            Ok(_) => return on_change(),
            Err(_) => return,
        };
        let mut candidates: Vec<String> = Vec::new();
        for event in &events {
            match classify(&roots, &event.path) {
                Event::Git => return on_change(),
                Event::Worktree(path) => candidates.push(path),
                Event::Noise => {}
            }
        }
        candidates.sort();
        candidates.dedup();
        if candidates.is_empty() {
            return;
        }
        let skipped = ignored(&check_dir, &candidates);
        if candidates.iter().any(|path| !skipped.contains(path)) {
            on_change();
        }
    })
    .map_err(|e| format!("cannot watch the working tree: {e}"))?;

    debouncer
        .watcher()
        .watch(&workdir, RecursiveMode::Recursive)
        .map_err(|e| format!("cannot watch {}: {e}", workdir.display()))?;
    Ok(RepoWatcher {
        _debouncer: debouncer,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;
    use std::time::Instant;

    use tempfile::TempDir;

    use super::*;
    use crate::git::open_repository;
    use crate::git::status_tests::{init_repo, write};
    use crate::git::tests::git;

    const DEBOUNCE: Duration = Duration::from_millis(100);

    fn watched(dir: &Path) -> (RepoWatcher, Arc<AtomicUsize>) {
        let (repo, _) = open_repository(dir).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let watcher = watch_worktree(&repo, DEBOUNCE, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        sleep(Duration::from_millis(400)); // let the file system watcher settle
        (watcher, calls)
    }

    fn repo() -> TempDir {
        let tmp = init_repo();
        write(tmp.path(), ".gitignore", "target/\n*.log\n");
        write(tmp.path(), "src/a.rs", "fn main() {}\n");
        git(tmp.path(), &["add", "-A"]);
        git(tmp.path(), &["commit", "-q", "-m", "first"]);
        tmp
    }

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

    fn settle() {
        sleep(Duration::from_millis(1500));
    }

    #[test]
    fn editing_creating_and_deleting_files_is_reported() {
        let tmp = repo();
        let dir = tmp.path();
        let (_watcher, calls) = watched(dir);

        write(dir, "src/a.rs", "fn main() { println!(); }\n");
        assert!(wait_for(&calls, 1), "edit not reported");
        settle();
        let n = calls.load(Ordering::SeqCst);
        write(dir, "src/new.rs", "// new\n");
        assert!(wait_for(&calls, n + 1), "new file not reported");
        settle();
        let n = calls.load(Ordering::SeqCst);
        fs::remove_file(dir.join("src/a.rs")).unwrap();
        assert!(wait_for(&calls, n + 1), "deletion not reported");
    }

    #[test]
    fn writes_to_ignored_paths_are_not_reported() {
        let tmp = repo();
        let dir = tmp.path();
        let (_watcher, calls) = watched(dir);

        // A build writing into an ignored folder, and an ignored file pattern.
        for i in 0..40 {
            write(dir, &format!("target/debug/deps/lib-{i}.rlib"), "binary\n");
        }
        write(dir, "build.log", "output\n");
        write(dir, "src/trace.log", "output\n");
        settle();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "ignored paths must not trigger a refresh"
        );

        // The watcher is still alive: a real change goes through.
        write(dir, "src/a.rs", "fn main() { 1; }\n");
        assert!(wait_for(&calls, 1));
    }

    #[test]
    fn a_change_to_gitignore_is_reported_and_takes_effect() {
        let tmp = repo();
        let dir = tmp.path();
        let (_watcher, calls) = watched(dir);

        write(dir, ".gitignore", "*.log\n"); // `target/` is no longer ignored
        assert!(wait_for(&calls, 1), ".gitignore change not reported");
        settle();
        let n = calls.load(Ordering::SeqCst);
        write(dir, "target/out.txt", "now visible\n");
        assert!(
            wait_for(&calls, n + 1),
            "a newly visible folder must be reported"
        );
    }

    #[test]
    fn staging_and_committing_are_reported_but_git_internals_are_not() {
        let tmp = repo();
        let dir = tmp.path();
        write(dir, "later.txt", "later\n");
        let (_watcher, calls) = watched(dir);

        git(dir, &["add", "later.txt"]); // the index changed
        assert!(wait_for(&calls, 1), "staging not reported");
        settle();
        let n = calls.load(Ordering::SeqCst);

        // Writing an object touches `.git/objects` only: not a change of the status.
        let status = std::process::Command::new("sh")
            .current_dir(dir)
            .args(["-c", "echo 'object content' | git hash-object -w --stdin"])
            .output()
            .unwrap();
        assert!(status.status.success());
        settle();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            n,
            "git internals must not trigger a refresh"
        );

        let n = calls.load(Ordering::SeqCst);
        git(dir, &["commit", "-q", "-m", "second"]);
        assert!(wait_for(&calls, n + 1), "commit not reported");
    }

    #[test]
    fn nothing_is_reported_after_the_watcher_is_dropped() {
        let tmp = repo();
        let (watcher, calls) = watched(tmp.path());
        drop(watcher);
        write(tmp.path(), "src/a.rs", "changed\n");
        settle();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn classifies_event_paths() {
        let roots = Roots::new(Path::new("/work/repo"), Path::new("/work/repo/.git"));
        let kind = |p: &str| match classify(&roots, Path::new(p)) {
            Event::Git => "git".to_string(),
            Event::Worktree(path) => format!("worktree:{path}"),
            Event::Noise => "noise".to_string(),
        };
        assert_eq!(kind("/work/repo/.git/index"), "git");
        assert_eq!(kind("/work/repo/.git/HEAD"), "git");
        assert_eq!(kind("/work/repo/.git/index.lock"), "noise");
        assert_eq!(kind("/work/repo/.git/objects/ab/cdef"), "noise");
        assert_eq!(kind("/work/repo/src/a.rs"), "worktree:src/a.rs");
        assert_eq!(kind("/work/repo"), "noise");
        assert_eq!(kind("/elsewhere/file"), "noise");
    }
}
