//! Fetch, pull, push, cancellation and the askpass helper. Remotes are local bare repositories:
//! no test reaches the network.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::cli::{ErrorKind, LongIo, RunOptions, answer_prompt, classify};
use super::ops::OpCtx;
use super::ops_tests::{git_out, repo_with};
use super::remote::*;
use super::status_tests::write;
use super::store::RepoStores;
use super::tests::git;

fn text(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_out(dir, args))
        .trim()
        .to_string()
}

fn ctx<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    OpCtx {
        workdir: tmp.path(),
        stores,
    }
}

/// Runs `action` with an `LongIo` that records progress and never gets asked anything.
fn with_io<T>(lines: &Mutex<Vec<String>>, action: impl FnOnce(&LongIo) -> T) -> T {
    let cancel = AtomicBool::new(false);
    let dir = TempDir::new().unwrap();
    let progress = |line: &str| lines.lock().unwrap().push(line.to_string());
    let prompt = |_: &str, _: &str| {};
    action(&LongIo {
        cancel: &cancel,
        askpass_dir: dir.path(),
        on_progress: &progress,
        on_prompt: &prompt,
    })
}

fn quiet<T>(action: impl FnOnce(&LongIo) -> T) -> T {
    with_io(&Mutex::new(Vec::new()), action)
}

/// A repository whose `origin` is a local bare repository holding `main`.
fn with_remote() -> (TempDir, TempDir) {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let url = remote.path().to_string_lossy().into_owned();
    git(tmp.path(), &["remote", "add", "origin", &url]);
    git(tmp.path(), &["push", "-q", "-u", "origin", "main"]);
    (tmp, remote)
}

/// Another clone of the remote, where "someone else" works.
fn colleague(remote: &TempDir) -> TempDir {
    let other = TempDir::new().unwrap();
    let url = remote.path().to_string_lossy().into_owned();
    git(other.path(), &["clone", "-q", &url, "."]);
    git(other.path(), &["config", "user.name", "Bob Fakename"]);
    git(
        other.path(),
        &["config", "user.email", "bob@example.invalid"],
    );
    git(other.path(), &["config", "commit.gpgsign", "false"]);
    other
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    write(dir, name, content);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn push_options(remote: Option<&str>, set_upstream: bool, force: bool) -> PushOptions {
    PushOptions {
        remote: remote.map(str::to_string),
        branch: None,
        set_upstream,
        force_with_lease: force,
    }
}

// ---------- classification ----------

#[test]
fn recognises_what_git_says_about_remotes() {
    let cases = [
        (
            "fatal: Authentication failed for 'https://example.invalid/x.git/'",
            ErrorKind::AuthFailed,
        ),
        (
            "git@example.invalid: Permission denied (publickey).",
            ErrorKind::AuthFailed,
        ),
        (
            "fatal: could not read Username for 'https://example.invalid': terminal prompts disabled",
            ErrorKind::AuthFailed,
        ),
        (
            "fatal: unable to access 'https://example.invalid/': Could not resolve host: example.invalid",
            ErrorKind::Unreachable,
        ),
        (
            "fatal: '/nowhere' does not appear to be a git repository",
            ErrorKind::Unreachable,
        ),
        (
            "fatal: The current branch topic has no upstream branch.",
            ErrorKind::NoUpstream,
        ),
        (
            "There is no tracking information for the current branch.",
            ErrorKind::NoUpstream,
        ),
        (
            "fatal: Need to specify how to reconcile divergent branches.",
            ErrorKind::DivergedBranches,
        ),
        (
            "fatal: Not possible to fast-forward, aborting.",
            ErrorKind::DivergedBranches,
        ),
        (
            " ! [rejected]        main -> main (non-fast-forward)\nerror: failed to push some refs to 'x'",
            ErrorKind::PushRejected,
        ),
        (
            "CONFLICT (content): Merge conflict in a.txt\nAutomatic merge failed",
            ErrorKind::Conflict,
        ),
        (
            "error: Your local changes to the following files would be overwritten by merge:\n\ta.txt",
            ErrorKind::WouldOverwriteChanges,
        ),
    ];
    for (message, kind) in cases {
        assert_eq!(classify("", message), Some(kind), "{message}");
    }
}

// ---------- fetch ----------

#[test]
fn fetch_brings_the_new_commits_and_reports_progress() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    commit_file(other.path(), "b.txt", "two\n", "from bob");
    git(other.path(), &["push", "-q", "origin", "main"]);

    let stores = RepoStores::default();
    let lines = Mutex::new(Vec::new());
    let result = with_io(&lines, |io| fetch(&ctx(&tmp, &stores), io, None, false)).unwrap();

    assert_eq!(
        text(tmp.path(), &["rev-parse", "origin/main"]),
        text(other.path(), &["rev-parse", "HEAD"])
    );
    assert_eq!(
        text(tmp.path(), &["rev-parse", "main"]),
        text(tmp.path(), &["rev-parse", "HEAD"]),
        "the local branch did not move"
    );
    assert!(result.summary.contains("main"), "{}", result.summary);
    assert!(
        lines.lock().unwrap().iter().any(|l| l.contains("objects")),
        "progress: {:?}",
        lines.lock().unwrap()
    );
    assert_eq!(
        stores.activity.list().last().unwrap().command,
        "git fetch --progress --all"
    );
}

#[test]
fn fetch_with_prune_forgets_deleted_remote_branches() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    git(other.path(), &["push", "-q", "origin", "main:old"]);
    let stores = RepoStores::default();
    quiet(|io| fetch(&ctx(&tmp, &stores), io, Some("origin"), false)).unwrap();
    assert!(text(tmp.path(), &["branch", "-r"]).contains("origin/old"));

    git(other.path(), &["push", "-q", "origin", "--delete", "old"]);
    quiet(|io| fetch(&ctx(&tmp, &stores), io, Some("origin"), false)).unwrap();
    assert!(text(tmp.path(), &["branch", "-r"]).contains("origin/old"));
    quiet(|io| fetch(&ctx(&tmp, &stores), io, Some("origin"), true)).unwrap();
    assert!(!text(tmp.path(), &["branch", "-r"]).contains("origin/old"));
}

#[test]
fn fetch_is_allowed_in_read_only_mode_but_push_is_not() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    quiet(|io| fetch(&ctx(&tmp, &stores), io, None, false)).unwrap();
    let error =
        quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, false))).unwrap_err();
    assert_eq!(error.kind, ErrorKind::ReadOnly);
    let error = quiet(|io| pull(&ctx(&tmp, &stores), io, None)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::ReadOnly);
}

#[test]
fn fetch_from_a_missing_remote_is_reported() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    git(
        tmp.path(),
        &["remote", "add", "origin", "/nowhere/at/all.git"],
    );
    let stores = RepoStores::default();
    let error = quiet(|io| fetch(&ctx(&tmp, &stores), io, None, false)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unreachable, "{}", error.message);
}

#[test]
fn lists_the_remotes() {
    let (tmp, _remote) = with_remote();
    git(tmp.path(), &["remote", "add", "backup", "/tmp/backup.git"]);
    let stores = RepoStores::default();
    let mut names = list_remotes(&ctx(&tmp, &stores)).unwrap();
    names.sort();
    assert_eq!(names, vec!["backup".to_string(), "origin".to_string()]);
}

// ---------- pull ----------

#[test]
fn pull_fast_forwards() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    commit_file(other.path(), "b.txt", "two\n", "from bob");
    git(other.path(), &["push", "-q", "origin", "main"]);

    let stores = RepoStores::default();
    let result = quiet(|io| pull(&ctx(&tmp, &stores), io, None)).unwrap();
    assert_eq!(
        text(tmp.path(), &["rev-parse", "HEAD"]),
        text(other.path(), &["rev-parse", "HEAD"])
    );
    assert!(tmp.path().join("b.txt").exists());
    assert!(result.status.staged.is_empty());
}

#[test]
fn diverged_branches_ask_for_a_strategy_and_each_one_works() {
    for (strategy, merge_commit) in [(PullStrategy::Merge, true), (PullStrategy::Rebase, false)] {
        let (tmp, remote) = with_remote();
        let other = colleague(&remote);
        commit_file(other.path(), "b.txt", "bob\n", "from bob");
        git(other.path(), &["push", "-q", "origin", "main"]);
        commit_file(tmp.path(), "c.txt", "alice\n", "from alice");

        let stores = RepoStores::default();
        let error = quiet(|io| pull(&ctx(&tmp, &stores), io, Some(PullStrategy::FastForwardOnly)))
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::DivergedBranches, "{}", error.message);

        quiet(|io| pull(&ctx(&tmp, &stores), io, Some(strategy))).unwrap();
        let parents = text(tmp.path(), &["rev-list", "--parents", "-n1", "HEAD"]);
        assert_eq!(
            parents.split_whitespace().count() == 3,
            merge_commit,
            "{strategy:?}: {parents}"
        );
        assert!(tmp.path().join("b.txt").exists() && tmp.path().join("c.txt").exists());
    }
}

#[test]
fn pull_without_an_upstream_is_reported() {
    let (tmp, _remote) = with_remote();
    git(tmp.path(), &["switch", "-q", "-c", "topic"]);
    let stores = RepoStores::default();
    let error = quiet(|io| pull(&ctx(&tmp, &stores), io, None)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NoUpstream, "{}", error.message);
}

#[test]
fn a_pull_that_conflicts_is_reported_and_leaves_the_merge_in_progress() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    commit_file(other.path(), "a.txt", "bob\n", "bob edits a");
    git(other.path(), &["push", "-q", "origin", "main"]);
    commit_file(tmp.path(), "a.txt", "alice\n", "alice edits a");

    let stores = RepoStores::default();
    let error = quiet(|io| pull(&ctx(&tmp, &stores), io, Some(PullStrategy::Merge))).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Conflict, "{}", error.message);
    assert!(tmp.path().join(".git/MERGE_HEAD").exists());
}

#[test]
fn a_pull_that_would_overwrite_local_changes_lists_the_files() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    commit_file(other.path(), "a.txt", "bob\n", "bob edits a");
    git(other.path(), &["push", "-q", "origin", "main"]);
    write(tmp.path(), "a.txt", "unsaved work\n");

    let stores = RepoStores::default();
    let error = quiet(|io| pull(&ctx(&tmp, &stores), io, None)).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::WouldOverwriteChanges,
        "{}",
        error.message
    );
    assert_eq!(error.files, vec!["a.txt".to_string()]);
    assert_eq!(
        fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
        "unsaved work\n"
    );
}

// ---------- push ----------

#[test]
fn push_sends_the_commits_to_the_upstream() {
    let (tmp, remote) = with_remote();
    commit_file(tmp.path(), "b.txt", "two\n", "second");
    let stores = RepoStores::default();
    quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, false))).unwrap();
    assert_eq!(
        text(remote.path(), &["rev-parse", "main"]),
        text(tmp.path(), &["rev-parse", "HEAD"])
    );
    assert_eq!(
        stores.activity.list().last().unwrap().command,
        "git push --progress"
    );
}

#[test]
fn a_new_branch_needs_a_remote_and_can_become_tracked() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["switch", "-q", "-c", "feature/x"]);
    commit_file(tmp.path(), "x.txt", "x\n", "work on x");
    let stores = RepoStores::default();

    let error =
        quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, false))).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NoUpstream, "{}", error.message);

    quiet(|io| {
        push(
            &ctx(&tmp, &stores),
            io,
            &push_options(Some("origin"), true, false),
        )
    })
    .unwrap();
    assert_eq!(
        text(remote.path(), &["rev-parse", "feature/x"]),
        text(tmp.path(), &["rev-parse", "HEAD"])
    );
    assert_eq!(
        text(
            tmp.path(),
            &["rev-parse", "--abbrev-ref", "feature/x@{upstream}"]
        ),
        "origin/feature/x"
    );
    assert_eq!(
        stores.activity.list().last().unwrap().command,
        "git push --progress -u origin feature/x"
    );
}

#[test]
fn a_rejected_push_can_be_forced_with_a_lease_but_only_if_nobody_pushed_since() {
    let (tmp, remote) = with_remote();
    let other = colleague(&remote);
    commit_file(other.path(), "b.txt", "bob\n", "from bob");
    git(other.path(), &["push", "-q", "origin", "main"]);
    commit_file(tmp.path(), "c.txt", "alice\n", "from alice");
    let stores = RepoStores::default();

    // Not a fast-forward.
    let error =
        quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, false))).unwrap_err();
    assert_eq!(error.kind, ErrorKind::PushRejected, "{}", error.message);

    // Bob's commit was never fetched: the lease protects it.
    let error =
        quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, true))).unwrap_err();
    assert_eq!(error.kind, ErrorKind::PushRejected, "{}", error.message);
    assert_eq!(
        text(remote.path(), &["rev-parse", "main"]),
        text(other.path(), &["rev-parse", "HEAD"]),
        "bob's work is still there"
    );

    // After fetching, the user has seen it: forcing is allowed.
    quiet(|io| fetch(&ctx(&tmp, &stores), io, None, false)).unwrap();
    quiet(|io| push(&ctx(&tmp, &stores), io, &push_options(None, false, true))).unwrap();
    assert_eq!(
        text(remote.path(), &["rev-parse", "main"]),
        text(tmp.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn deletes_a_branch_on_the_remote() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["push", "-q", "origin", "main:gone"]);
    let stores = RepoStores::default();
    quiet(|io| delete_remote_branch(&ctx(&tmp, &stores), io, "origin", "gone")).unwrap();
    assert!(!text(remote.path(), &["branch"]).contains("gone"));
    assert!(
        stores
            .activity
            .list()
            .last()
            .unwrap()
            .command
            .ends_with("push --progress origin --delete gone")
    );
}

#[test]
fn names_that_look_like_options_are_refused_before_git_runs() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    let error =
        quiet(|io| delete_remote_branch(&ctx(&tmp, &stores), io, "origin", "--all")).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidName);
    let error =
        quiet(|io| fetch(&ctx(&tmp, &stores), io, Some("--upload-pack=x"), false)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidName);
    assert!(stores.activity.list().is_empty());
}

// ---------- cancelling and asking ----------

#[test]
fn a_running_fetch_can_be_cancelled() {
    let (tmp, _remote) = with_remote();
    // A remote that never answers: the "upload-pack" is a script that sleeps.
    let slow = tmp.path().join(".git").join("slow-upload-pack.sh");
    fs::write(&slow, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&slow, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        tmp.path(),
        &[
            "config",
            "remote.origin.uploadpack",
            // Run through `sh` with slashes, so it works wherever git runs its shell.
            &format!("sh '{}'", slow.to_string_lossy().replace('\\', "/")),
        ],
    );

    let stores = RepoStores::default();
    let cancel = AtomicBool::new(false);
    let dir = TempDir::new().unwrap();
    let progress = |_: &str| {};
    let prompt = |_: &str, _: &str| {};
    let io = LongIo {
        cancel: &cancel,
        askpass_dir: dir.path(),
        on_progress: &progress,
        on_prompt: &prompt,
    };
    let started = Instant::now();
    let error = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            cancel.store(true, Ordering::SeqCst);
        });
        fetch(&ctx(&tmp, &stores), &io, None, false).unwrap_err()
    });
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert!(
        !tmp.path().join(".git/shallow.lock").exists(),
        "git cleaned up after itself"
    );
}

/// `git credential fill` asks for a username and a password through the askpass helper, with
/// no network: a faithful stand-in for what an HTTPS remote does.
fn credential_fill(answers: &[Option<&str>]) -> (Vec<String>, super::cli::RunOutput) {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let cancel = AtomicBool::new(false);
    let dir = TempDir::new().unwrap();
    let asked = Mutex::new(Vec::<String>::new());
    let progress = |_: &str| {};
    let prompt = |id: &str, prompt: &str| {
        let mut asked = asked.lock().unwrap();
        let answer = answers.get(asked.len()).copied().flatten();
        asked.push(prompt.to_string());
        answer_prompt(dir.path(), id, answer).unwrap();
    };
    let io = LongIo {
        cancel: &cancel,
        askpass_dir: dir.path(),
        on_progress: &progress,
        on_prompt: &prompt,
    };
    let output = stores
        .runner()
        .run_long(
            tmp.path(),
            &["-c", "credential.helper=", "credential", "fill"],
            RunOptions::with_stdin(b"protocol=https\nhost=example.invalid\n\n".to_vec()),
            &io,
        )
        .unwrap();
    let asked = asked.into_inner().unwrap();
    (asked, output)
}

#[test]
fn questions_from_git_reach_the_interface_and_the_answers_come_back() {
    let (asked, output) = credential_fill(&[Some("alice"), Some("s3cret pass")]);
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked[0].starts_with("Username for"), "{asked:?}");
    assert!(asked[1].starts_with("Password for"), "{asked:?}");
    let out = output.stdout_text();
    assert!(out.contains("username=alice"), "{out}");
    assert!(out.contains("password=s3cret pass"), "{out}");
}

#[test]
fn refusing_a_question_makes_the_command_fail() {
    let (asked, output) = credential_fill(&[None]);
    assert_eq!(asked.len(), 1);
    assert_ne!(output.code, 0);
}

#[test]
fn answers_to_unknown_ids_are_refused() {
    let dir = TempDir::new().unwrap();
    assert!(answer_prompt(dir.path(), "../x", Some("a")).is_err());
    assert!(answer_prompt(dir.path(), "", Some("a")).is_err());
}

#[test]
fn remote_urls_lists_each_remote_once_with_its_fetch_address() {
    let (tmp, remote) = with_remote();
    git(
        tmp.path(),
        &["remote", "add", "azure", "git@ssh.dev.azure.com:v3/o/p/r"],
    );
    let stores = RepoStores::default();
    let urls = remote_urls(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(urls.len(), 2);
    assert_eq!(urls[0].name, "azure");
    assert_eq!(urls[0].url, "git@ssh.dev.azure.com:v3/o/p/r");
    assert_eq!(urls[1].name, "origin");
    assert_eq!(urls[1].url, remote.path().to_string_lossy());
    assert_eq!(urls[0].host.as_deref(), Some("ssh.dev.azure.com"));
}

#[test]
fn host_of_url_reads_every_shape_of_address() {
    assert_eq!(
        host_of_url("git@work-github:team/tool.git").as_deref(),
        Some("work-github")
    );
    assert_eq!(
        host_of_url("ssh://git@Host.Example:2222/team/app.git").as_deref(),
        Some("host.example")
    );
    assert_eq!(
        host_of_url("https://user@github.com/acme/tool").as_deref(),
        Some("github.com")
    );
    assert_eq!(host_of_url("/local/path/repo"), None);
    assert_eq!(host_of_url("file:///tmp/repo"), None);
    assert_eq!(host_of_url("git@host:path://odd"), Some("host".to_string()));
}

#[test]
fn an_ssh_alias_that_is_not_valid_is_returned_untouched() {
    // Never handed to ssh: a name starting with a dash would be read as an option.
    assert_eq!(resolve_ssh_host("-oProxyCommand=x"), "-oProxyCommand=x");
    assert_eq!(resolve_ssh_host(""), "");
}

#[test]
fn parse_remote_urls_ignores_push_lines_and_noise() {
    let text =
        "origin\thttps://a/b (fetch)\norigin\thttps://a/b (push)\nweird line\nup\tx (fetch)\n";
    let urls = parse_remote_urls(text);
    assert_eq!(
        urls.iter().map(|u| u.name.as_str()).collect::<Vec<_>>(),
        ["origin", "up"]
    );
}

// ---------- managing the remotes ----------

fn url_of(remotes: &[RemoteUrl], name: &str) -> Option<String> {
    remotes
        .iter()
        .find(|r| r.name == name)
        .map(|r| r.url.clone())
}

#[test]
fn a_remote_is_added_and_listed_with_its_address() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let remotes = add_remote(
        &ctx(&tmp, &stores),
        "upstream",
        " https://example.invalid/team/tool.git ",
    )
    .unwrap();
    assert_eq!(
        url_of(&remotes, "upstream").as_deref(),
        Some("https://example.invalid/team/tool.git")
    );
    let again = add_remote(
        &ctx(&tmp, &stores),
        "upstream",
        "https://example.invalid/x.git",
    );
    assert!(again.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn a_remote_is_not_added_with_a_bad_name_or_address() {
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let c = ctx(&tmp, &stores);
    for name in ["", "  ", "-x"] {
        assert!(
            add_remote(&c, name, "https://example.invalid/r.git").is_err(),
            "{name:?}"
        );
    }
    for url in ["", "  ", "--upload-pack=touch evil", "a\nb"] {
        assert!(add_remote(&c, "ok", url).is_err(), "{url:?}");
    }
    assert!(remote_urls(&c).unwrap().is_empty());
}

#[test]
fn renaming_a_remote_moves_its_remote_tracking_branches() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    let c = ctx(&tmp, &stores);
    let remotes = rename_remote(&c, "origin", "upstream").unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "upstream");
    assert!(
        !text(tmp.path(), &["for-each-ref", "refs/remotes/upstream/main"]).is_empty(),
        "the remote-tracking branch follows the remote"
    );
    assert!(text(tmp.path(), &["for-each-ref", "refs/remotes/origin"]).is_empty());
    assert_eq!(
        text(tmp.path(), &["config", "branch.main.remote"]),
        "upstream"
    );
}

#[test]
fn renaming_to_a_taken_or_unknown_name_is_refused() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    let c = ctx(&tmp, &stores);
    add_remote(&c, "backup", "https://example.invalid/b.git").unwrap();
    assert!(rename_remote(&c, "origin", "backup").is_err());
    assert!(rename_remote(&c, "nowhere", "x").is_err());
    // Renaming to itself changes nothing and is not an error.
    assert_eq!(rename_remote(&c, "origin", "origin").unwrap().len(), 2);
}

#[test]
fn a_remote_address_can_be_changed() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    let c = ctx(&tmp, &stores);
    let remotes = set_remote_url(&c, "origin", "git@example.invalid:team/new.git").unwrap();
    assert_eq!(
        url_of(&remotes, "origin").as_deref(),
        Some("git@example.invalid:team/new.git")
    );
    assert!(set_remote_url(&c, "nowhere", "https://example.invalid/x.git").is_err());
    assert!(set_remote_url(&c, "origin", "-bad").is_err());
}

#[test]
fn removing_a_remote_drops_its_remote_tracking_branches() {
    let (tmp, remote) = with_remote();
    let stores = RepoStores::default();
    let c = ctx(&tmp, &stores);
    let remotes = remove_remote(&c, "origin").unwrap();
    assert!(remotes.is_empty());
    assert!(text(tmp.path(), &["for-each-ref", "refs/remotes"]).is_empty());
    assert!(remove_remote(&c, "origin").is_err());
    // The server is untouched.
    assert!(!text(remote.path(), &["branch", "--list", "main"]).is_empty());
}

#[test]
fn managing_the_remotes_is_refused_in_read_only_mode() {
    let (tmp, _remote) = with_remote();
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let c = ctx(&tmp, &stores);
    assert!(add_remote(&c, "x", "https://example.invalid/x.git").is_err());
    assert!(rename_remote(&c, "origin", "y").is_err());
    assert!(set_remote_url(&c, "origin", "https://example.invalid/y.git").is_err());
    assert!(remove_remote(&c, "origin").is_err());
    assert!(quiet(|io| push_tags(&c, io, "origin", None)).is_err());
    assert_eq!(remote_urls(&c).unwrap().len(), 1);
}

// ---------- pushing tags ----------

#[test]
fn one_tag_is_pushed_and_the_others_stay_local() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["tag", "v1"]);
    git(tmp.path(), &["tag", "v2"]);
    let stores = RepoStores::default();
    quiet(|io| push_tags(&ctx(&tmp, &stores), io, "origin", Some("v1"))).unwrap();
    let on_remote = text(remote.path(), &["tag", "--list"]);
    assert_eq!(on_remote, "v1");
}

#[test]
fn every_tag_is_pushed_at_once() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["tag", "v1"]);
    git(tmp.path(), &["tag", "-a", "-m", "second", "v2"]);
    let stores = RepoStores::default();
    quiet(|io| push_tags(&ctx(&tmp, &stores), io, "origin", None)).unwrap();
    assert_eq!(text(remote.path(), &["tag", "--list"]), "v1\nv2");
}

#[test]
fn a_tag_name_that_is_a_refspec_or_an_option_is_refused() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["tag", "v1"]);
    let stores = RepoStores::default();
    for name in ["v1:refs/heads/hijack", "-f", "a b", ""] {
        assert!(
            quiet(|io| push_tags(&ctx(&tmp, &stores), io, "origin", Some(name))).is_err(),
            "{name:?}"
        );
    }
    assert!(quiet(|io| push_tags(&ctx(&tmp, &stores), io, "-x", None)).is_err());
    assert!(text(remote.path(), &["branch", "--list", "hijack"]).is_empty());
}

#[test]
fn a_tag_the_remote_has_elsewhere_is_refused_not_forced() {
    let (tmp, remote) = with_remote();
    git(tmp.path(), &["tag", "v1"]);
    let stores = RepoStores::default();
    quiet(|io| push_tags(&ctx(&tmp, &stores), io, "origin", Some("v1"))).unwrap();
    write(tmp.path(), "b.txt", "two\n");
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-q", "-m", "second"]);
    git(tmp.path(), &["tag", "-f", "v1"]);
    let before = text(remote.path(), &["rev-parse", "v1"]);
    assert!(quiet(|io| push_tags(&ctx(&tmp, &stores), io, "origin", Some("v1"))).is_err());
    assert_eq!(text(remote.path(), &["rev-parse", "v1"]), before);
}

// ---------- cloning ----------

#[test]
fn a_clone_creates_the_folder_with_the_files() {
    let (_tmp, remote) = with_remote();
    let parent = TempDir::new().unwrap();
    let url = remote.path().to_string_lossy().into_owned();
    let path = quiet(|io| clone_repository(io, &url, parent.path(), "copy")).unwrap();
    assert_eq!(path, parent.path().join("copy"));
    assert!(path.join("a.txt").exists());
}

#[test]
fn a_clone_goes_into_an_existing_empty_folder_but_not_a_full_one() {
    let (_tmp, remote) = with_remote();
    let parent = TempDir::new().unwrap();
    let url = remote.path().to_string_lossy().into_owned();
    fs::create_dir(parent.path().join("empty")).unwrap();
    quiet(|io| clone_repository(io, &url, parent.path(), "empty")).unwrap();
    assert!(parent.path().join("empty").join("a.txt").exists());

    fs::create_dir(parent.path().join("full")).unwrap();
    fs::write(parent.path().join("full").join("x.txt"), "x").unwrap();
    let refused = quiet(|io| clone_repository(io, &url, parent.path(), "full")).unwrap_err();
    assert!(refused.to_string().contains("not empty"));
    assert!(parent.path().join("full").join("x.txt").exists());
}

#[test]
fn a_clone_refuses_what_could_leave_the_folder_or_be_read_as_an_option() {
    let parent = TempDir::new().unwrap();
    for name in ["", ".", "..", "a/b", "..\\x"] {
        assert!(
            quiet(|io| clone_repository(io, "https://x.invalid/r", parent.path(), name)).is_err()
        );
    }
    for url in ["", "  ", "--upload-pack=touch evil"] {
        assert!(quiet(|io| clone_repository(io, url, parent.path(), "copy")).is_err());
    }
    assert!(!parent.path().join("copy").exists());
    let missing = parent.path().join("nowhere");
    assert!(quiet(|io| clone_repository(io, "https://x.invalid/r", &missing, "copy")).is_err());
}

#[test]
fn a_failed_clone_leaves_no_folder_behind() {
    let parent = TempDir::new().unwrap();
    let nothing = parent.path().join("no-such-repository");
    let url = nothing.to_string_lossy().into_owned();
    assert!(quiet(|io| clone_repository(io, &url, parent.path(), "copy")).is_err());
    assert!(!parent.path().join("copy").exists());
}

// ---------- the fetch made in the background ----------

#[test]
fn auto_fetch_tells_which_tracked_branch_received_commits() {
    use super::autofetch::auto_fetch;
    let (tmp, remote) = with_remote();
    let stores = RepoStores::default();

    let nothing = auto_fetch(&ctx(&tmp, &stores)).unwrap();
    assert!(nothing.updates.is_empty());
    assert_eq!(nothing.behind_total, 0);

    let bob = colleague(&remote);
    commit_file(bob.path(), "b.txt", "2\n", "Bob one");
    commit_file(bob.path(), "c.txt", "3\n", "Bob two");
    git(bob.path(), &["push", "-q", "origin", "main"]);

    let found = auto_fetch(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(found.behind_total, 2);
    assert_eq!(found.updates.len(), 1);
    assert_eq!(found.updates[0].branch, "main");
    assert_eq!(found.updates[0].upstream, "origin/main");
    assert_eq!(found.updates[0].new_commits, 2);

    // Nothing new since: the commits still wait, but they are not news.
    let again = auto_fetch(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(again.behind_total, 2);
    assert!(again.updates.is_empty());
}

#[test]
fn auto_fetch_leaves_a_repository_without_remote_alone() {
    use super::autofetch::auto_fetch;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    let stores = RepoStores::default();
    let result = auto_fetch(&ctx(&tmp, &stores)).unwrap();
    assert_eq!(result.behind_total, 0);
    assert!(result.updates.is_empty());
}

#[test]
fn auto_fetch_fails_instead_of_asking_when_the_remote_is_unreachable() {
    use super::autofetch::auto_fetch;
    let tmp = repo_with(&[("a.txt", "one\n")]);
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "/nonexistent/path/for/a/test.git",
        ],
    );
    let stores = RepoStores::default();
    assert!(auto_fetch(&ctx(&tmp, &stores)).is_err());
}
