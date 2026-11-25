//! The one place that starts the `git` binary.
//!
//! Every write goes through [`Runner`]: same behaviour as the terminal (hooks, signing,
//! configuration, credentials), a controlled environment, typed errors, and an entry in the
//! activity log.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::activity::ActivityLog;

/// What went wrong, so the interface can react with the right dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// The repository is in read-only mode: nothing was run.
    ReadOnly,
    /// Another git process holds a lock (`index.lock`...). The file is never removed.
    Locked,
    /// `user.name` or `user.email` is missing.
    NeedsIdentity,
    /// A hook (pre-commit, commit-msg...) rejected the commit.
    HookFailed,
    /// Signing the commit failed.
    SigningFailed,
    /// There is nothing to commit.
    NothingToCommit,
    /// The file changed since the hunk was shown: reload the diff.
    StaleHunk,
    /// The name is not a valid branch or tag name.
    InvalidName,
    /// A branch or tag with that name exists already.
    AlreadyExists,
    /// Switching would overwrite local changes (the files are in `GitError::files`).
    WouldOverwriteChanges,
    /// The branch has commits no other branch has: deleting it needs confirmation.
    NotFullyMerged,
    /// The branch is checked out (here or in another working tree).
    BranchCheckedOut,
    /// The remote refused the credentials, or there were none to give.
    AuthFailed,
    /// The remote could not be reached (name, network, path).
    Unreachable,
    /// The remote refused the push (not a fast-forward, a stale lease, a server hook).
    PushRejected,
    /// The branch has no upstream to push to or pull from.
    NoUpstream,
    /// The branches diverged and git was not told how to reconcile them.
    DivergedBranches,
    /// A merge or a rebase stopped on conflicts.
    Conflict,
    /// The user cancelled a long operation.
    Cancelled,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitError {
    pub kind: ErrorKind,
    /// What git said, or what the app wants to tell.
    pub message: String,
    /// Files the error is about (those a checkout would overwrite); empty otherwise.
    pub files: Vec<String>,
}

impl GitError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            files: Vec::new(),
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Other, message)
    }
}

impl From<String> for GitError {
    fn from(message: String) -> Self {
        Self::other(message)
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub struct RunOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub code: i32,
}

impl RunOutput {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// What to show when the command failed: git's complaint, else its output.
    pub fn failure_text(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout_text()
        } else {
            self.stderr.clone()
        };
        text.trim().to_string()
    }
}

#[derive(Default)]
pub struct RunOptions {
    /// Bytes written to the standard input of git (it is closed otherwise).
    pub stdin: Option<Vec<u8>>,
    /// A background read (status...): it never takes optional locks, so it cannot get in the
    /// way of the user's own git commands, and it is not logged.
    pub background: bool,
    /// Extra environment for this command only.
    pub env: Vec<(String, String)>,
}

impl RunOptions {
    pub fn background() -> Self {
        Self {
            background: true,
            ..Self::default()
        }
    }

    pub fn with_stdin(stdin: Vec<u8>) -> Self {
        Self {
            stdin: Some(stdin),
            ..Self::default()
        }
    }

    pub fn with_env(env: Vec<(String, String)>) -> Self {
        Self {
            env,
            ..Self::default()
        }
    }
}

/// Starts git in a working directory.
pub struct Runner<'a> {
    pub log: &'a ActivityLog,
    /// Extra environment, for tests.
    pub env: &'a [(String, String)],
}

impl Runner<'_> {
    /// The command with the controlled environment, ready to be started.
    fn command(&self, dir: &Path, args: &[&str], options: &RunOptions) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(dir)
            .args(args)
            // Parsable messages, no prompts, no pager, no editor.
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("GIT_EDITOR", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if options.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        if options.background {
            command.env("GIT_OPTIONAL_LOCKS", "0");
        }
        command.env("PATH", tool_path());
        for (key, value) in self.env {
            command.env(key, value);
        }
        for (key, value) in &options.env {
            command.env(key, value);
        }
        command
    }

    /// Runs `git <args>` and returns its output whatever the exit code.
    pub fn run_raw(
        &self,
        dir: &Path,
        args: &[&str],
        options: RunOptions,
    ) -> Result<RunOutput, GitError> {
        let mut command = self.command(dir, args, &options);

        let started = SystemTime::now();
        let timer = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|e| GitError::other(format!("cannot start git: {e}")))?;
        if let (Some(bytes), Some(mut stdin)) = (options.stdin.as_ref(), child.stdin.take()) {
            // A command that exits early closes the pipe: that is not an error of ours.
            let _ = stdin.write_all(bytes);
        }
        let output = child
            .wait_with_output()
            .map_err(|e| GitError::other(format!("git did not finish: {e}")))?;
        let result = RunOutput {
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code().unwrap_or(-1),
        };

        if !options.background {
            let shown = if options.stdin.is_some() {
                format!(
                    "{} < [{} bytes]",
                    display(args),
                    options.stdin.as_ref().map_or(0, Vec::len)
                )
            } else {
                display(args)
            };
            self.log.record(
                shown,
                started,
                timer.elapsed().as_millis() as u64,
                result.code,
                &result.stdout,
                &result.stderr,
            );
        }
        Ok(result)
    }

    /// Runs `git <args>`; a non-zero exit code becomes an error classified from git's message.
    pub fn run(
        &self,
        dir: &Path,
        args: &[&str],
        options: RunOptions,
    ) -> Result<RunOutput, GitError> {
        let output = self.run_raw(dir, args, options)?;
        if output.code == 0 {
            return Ok(output);
        }
        Err(error_from(&output))
    }
}

/// How a long operation (fetch, pull, push) talks to the interface while it runs.
pub struct LongIo<'a> {
    /// Set to stop the command: git is asked to terminate, then killed if it does not.
    pub cancel: &'a AtomicBool,
    /// Directory the askpass helper uses to ask questions and get answers.
    pub askpass_dir: &'a Path,
    /// Called for each progress line git prints (`Receiving objects: 45% ...`).
    pub on_progress: &'a (dyn Fn(&str) + Sync),
    /// Called once per question git or ssh asks (a password, a passphrase, a host key): the
    /// prompt id and the text. Answer with [`answer_prompt`].
    pub on_prompt: &'a (dyn Fn(&str, &str) + Sync),
}

/// The helper git and ssh start when they need a secret. It writes the question to a file and
/// waits for the app to write the answer next to it, so no extra program has to be shipped.
const ASKPASS_SCRIPT: &str = r#"#!/bin/sh
dir="${GIT_HYDRA_ASKPASS_DIR:?}"
id="$$"
printf '%s' "$1" > "$dir/$id.prompt.tmp" && mv "$dir/$id.prompt.tmp" "$dir/$id.prompt"
while [ ! -f "$dir/$id.answer" ] && [ ! -f "$dir/$id.cancel" ]; do
  kill -0 "$PPID" 2>/dev/null || exit 1
  sleep 0.1
done
if [ -f "$dir/$id.cancel" ]; then
  exit 1
fi
cat "$dir/$id.answer"
"#;

/// Writes the askpass helper into `dir` and returns the environment that makes git and ssh use it.
fn askpass_env(dir: &Path) -> Result<Vec<(String, String)>, GitError> {
    let fail =
        |e: std::io::Error| GitError::other(format!("cannot prepare the askpass helper: {e}"));
    std::fs::create_dir_all(dir).map_err(fail)?;
    let script: PathBuf = dir.join("askpass.sh");
    std::fs::write(&script, ASKPASS_SCRIPT).map_err(fail)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).map_err(fail)?;
    }
    let script = script.to_string_lossy().into_owned();
    Ok(vec![
        ("GIT_ASKPASS".into(), script.clone()),
        ("SSH_ASKPASS".into(), script),
        ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
        (
            "GIT_HYDRA_ASKPASS_DIR".into(),
            dir.to_string_lossy().into_owned(),
        ),
        // Older ssh only uses SSH_ASKPASS when it believes a display exists.
        (
            "DISPLAY".into(),
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        ),
    ])
}

/// Answers a question asked through the askpass helper; `None` refuses (the command then fails).
pub fn answer_prompt(dir: &Path, prompt_id: &str, answer: Option<&str>) -> Result<(), GitError> {
    if prompt_id.is_empty() || !prompt_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(GitError::other("invalid prompt id"));
    }
    let fail = |e: std::io::Error| GitError::other(format!("cannot answer: {e}"));
    match answer {
        Some(text) => {
            let temp = dir.join(format!("{prompt_id}.answer.tmp"));
            std::fs::write(&temp, text).map_err(fail)?;
            std::fs::rename(&temp, dir.join(format!("{prompt_id}.answer"))).map_err(fail)
        }
        None => std::fs::write(dir.join(format!("{prompt_id}.cancel")), "").map_err(fail),
    }
}

/// Reads git's progress stream: every update goes to `on_progress`; the text returned holds
/// only the lines git ended for good (`done.`, errors), not the intermediate percentages.
fn read_progress(mut stream: impl Read, on_progress: &(dyn Fn(&str) + Sync)) -> String {
    let mut kept = Vec::new();
    let mut line = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &chunk[..read] {
            if byte == b'\r' || byte == b'\n' {
                let text = String::from_utf8_lossy(&line).trim().to_string();
                if !text.is_empty() {
                    on_progress(&text);
                }
                if byte == b'\n' {
                    kept.extend_from_slice(&line);
                    kept.push(b'\n');
                }
                line.clear();
            } else {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        kept.extend_from_slice(&line);
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// Sends a signal to the whole process group of a long command (git and what it started, such
/// as ssh or a hook): TERM first, so git cleans up its lock files, KILL if that is not enough.
fn signal_group(pid: u32, signal: &str) {
    let _ = Command::new("kill")
        .args([signal, "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn report_new_prompts(io: &LongIo, seen: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(io.askpass_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".prompt") else {
            continue;
        };
        if seen.insert(id.to_string()) {
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            (io.on_prompt)(id, &text);
        }
    }
}

impl Runner<'_> {
    /// Runs a command that can take a long time: it reports progress, can ask questions
    /// through the askpass helper, and stops when `io.cancel` is set (error `Cancelled`).
    pub fn run_long(
        &self,
        dir: &Path,
        args: &[&str],
        mut options: RunOptions,
        io: &LongIo,
    ) -> Result<RunOutput, GitError> {
        options.env.extend(askpass_env(io.askpass_dir)?);
        let mut command = self.command(dir, args, &options);
        // Its own process group, so cancelling reaches everything git started.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let started = SystemTime::now();
        let timer = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|e| GitError::other(format!("cannot start git: {e}")))?;
        if let (Some(bytes), Some(mut stdin)) = (options.stdin.as_ref(), child.stdin.take()) {
            let _ = stdin.write_all(bytes);
        }
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let mut cancelled = false;
        let mut waited: Result<i32, GitError> = Ok(-1);
        let (out_bytes, err_text) = std::thread::scope(|scope| {
            let out = scope.spawn(move || {
                let mut buffer = Vec::new();
                if let Some(mut stdout) = stdout {
                    let _ = stdout.read_to_end(&mut buffer);
                }
                buffer
            });
            let err = scope.spawn(move || match stderr {
                Some(stderr) => read_progress(stderr, io.on_progress),
                None => String::new(),
            });

            let mut seen: HashSet<String> = HashSet::new();
            let mut terminated_at: Option<Instant> = None;
            waited = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status.code().unwrap_or(-1)),
                    Ok(None) => {}
                    Err(e) => break Err(GitError::other(format!("git did not finish: {e}"))),
                }
                if io.cancel.load(Ordering::SeqCst) {
                    match terminated_at {
                        None => {
                            cancelled = true;
                            terminated_at = Some(Instant::now());
                            signal_group(child.id(), "-TERM");
                            // Refuse any question that is waiting for an answer.
                            for id in &seen {
                                let _ = answer_prompt(io.askpass_dir, id, None);
                            }
                        }
                        Some(at) if at.elapsed() > Duration::from_secs(3) => {
                            signal_group(child.id(), "-KILL");
                            let _ = child.kill();
                        }
                        Some(_) => {}
                    }
                } else {
                    report_new_prompts(io, &mut seen);
                }
                std::thread::sleep(Duration::from_millis(40));
            };
            (
                out.join().unwrap_or_default(),
                err.join().unwrap_or_default(),
            )
        });
        let code = waited?;

        self.log.record(
            display(args),
            started,
            timer.elapsed().as_millis() as u64,
            code,
            &out_bytes,
            &err_text,
        );
        if cancelled {
            return Err(GitError::new(ErrorKind::Cancelled, "Cancelled."));
        }
        Ok(RunOutput {
            stdout: out_bytes,
            stderr: err_text,
            code,
        })
    }

    /// [`run_long`](Self::run_long), with a non-zero exit code turned into a classified error.
    pub fn run_long_checked(
        &self,
        dir: &Path,
        args: &[&str],
        options: RunOptions,
        io: &LongIo,
    ) -> Result<RunOutput, GitError> {
        let output = self.run_long(dir, args, options, io)?;
        if output.code == 0 {
            return Ok(output);
        }
        Err(error_from(&output))
    }
}

/// Folders where the tools git calls (hooks, `gpg`, `ssh`, `node`...) usually are on a Mac.
const COMMON_TOOL_DIRS: [&str; 4] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

/// `path` with the usual tool folders added at the end when they are missing. An application
/// started from the Finder gets a short `PATH` without them, so hooks and commit signing that
/// work in a terminal would fail from the app (a rebase replays every commit, hooks and
/// signing included).
pub fn with_common_tool_dirs(path: &str) -> String {
    let mut parts: Vec<&str> = path.split(':').filter(|p| !p.is_empty()).collect();
    for dir in COMMON_TOOL_DIRS {
        if !parts.contains(&dir) {
            parts.push(dir);
        }
    }
    parts.join(":")
}

fn tool_path() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| with_common_tool_dirs(&std::env::var("PATH").unwrap_or_default()))
}

/// The command as it would be typed.
fn display(args: &[&str]) -> String {
    let quoted: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.is_empty()
                || arg.contains(|c: char| c.is_whitespace() || "\"'$&|;<>()*?[]{}\\`".contains(c))
            {
                format!("'{}'", arg.replace('\'', "'\\''"))
            } else {
                (*arg).to_string()
            }
        })
        .collect();
    format!("git {}", quoted.join(" "))
}

/// The error for a failed command (no knowledge of which command it was beyond its output).
pub fn error_from(output: &RunOutput) -> GitError {
    let text = output.failure_text();
    let kind = classify(&output.stdout_text(), &output.stderr).unwrap_or(ErrorKind::Other);
    let error = GitError::new(kind, text);
    if kind == ErrorKind::WouldOverwriteChanges {
        let files = indented_lines(&output.stderr);
        return error.with_files(files);
    }
    error
}

/// The lines git indents with a tab: the files it lists after a complaint.
pub fn indented_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix('\t'))
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Recognises the failures the interface treats specially, from the messages git prints
/// (the runner sets `LC_ALL=C`, so they are in English).
pub fn classify(stdout: &str, stderr: &str) -> Option<ErrorKind> {
    let both = format!("{stderr}\n{stdout}");
    let has = |needle: &str| both.contains(needle);
    if has(".lock': File exists") || has("Another git process seems to be running") {
        return Some(ErrorKind::Locked);
    }
    if has("Author identity unknown")
        || has("Committer identity unknown")
        || has("Please tell me who you are")
        || has("unable to auto-detect email address")
        || has("empty ident name")
    {
        return Some(ErrorKind::NeedsIdentity);
    }
    if has("gpg failed to sign the data")
        || has("failed to sign the data")
        || has("Couldn't load public key")
    {
        return Some(ErrorKind::SigningFailed);
    }
    // Before "nothing to commit": a conflicted stash or merge prints that too.
    if has("CONFLICT (")
        || has("Automatic merge failed")
        || has("could not apply")
        || has("Unmerged paths")
    {
        return Some(ErrorKind::Conflict);
    }
    if has("nothing to commit")
        || has("no changes added to commit")
        || has("nothing added to commit")
    {
        return Some(ErrorKind::NothingToCommit);
    }
    let lower = both.to_lowercase();
    let mentions = |needle: &str| lower.contains(needle);
    if mentions("would be overwritten by checkout")
        || mentions("would be overwritten by switch")
        || mentions("would be overwritten by merge")
        || mentions("would be overwritten by cherry-pick")
        || mentions("would be overwritten by revert")
        || mentions("cannot rebase: you have unstaged changes")
        || mentions("cannot rebase: your index contains uncommitted changes")
        || mentions("please commit or stash them")
    {
        return Some(ErrorKind::WouldOverwriteChanges);
    }
    if mentions("authentication failed")
        || mentions("permission denied (publickey")
        || mentions("could not read username")
        || mentions("could not read password")
        || mentions("terminal prompts disabled")
        || mentions("host key verification failed")
    {
        return Some(ErrorKind::AuthFailed);
    }
    if mentions("could not resolve host")
        || mentions("does not appear to be a git repository")
        || mentions("unable to access")
        || mentions("connection refused")
        || mentions("connection timed out")
        || mentions("repository not found")
        || mentions("no such remote")
    {
        return Some(ErrorKind::Unreachable);
    }
    if mentions("has no upstream branch")
        || mentions("no tracking information for the current branch")
        || mentions("no upstream configured")
    {
        return Some(ErrorKind::NoUpstream);
    }
    if mentions("need to specify how to reconcile divergent branches")
        || mentions("not possible to fast-forward")
    {
        return Some(ErrorKind::DivergedBranches);
    }
    if mentions("[rejected]")
        || mentions("[remote rejected]")
        || mentions("failed to push some refs")
    {
        return Some(ErrorKind::PushRejected);
    }
    if mentions("is not fully merged") {
        return Some(ErrorKind::NotFullyMerged);
    }
    if mentions("cannot delete branch") && (mentions("checked out") || mentions("used by worktree"))
    {
        return Some(ErrorKind::BranchCheckedOut);
    }
    if mentions("already exists") && (mentions("a branch named") || mentions("tag '")) {
        return Some(ErrorKind::AlreadyExists);
    }
    if mentions("is not a valid branch name") || mentions("is not a valid tag name") {
        return Some(ErrorKind::InvalidName);
    }
    None
}
