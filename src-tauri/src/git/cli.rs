//! The one place that starts the `git` binary.
//!
//! Every write goes through [`Runner`]: same behaviour as the terminal (hooks, signing,
//! configuration, credentials), a controlled environment, typed errors, and an entry in the
//! activity log.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime};

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
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitError {
    pub kind: ErrorKind,
    /// What git said, or what the app wants to tell.
    pub message: String,
}

impl GitError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
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
    /// Runs `git <args>` and returns its output whatever the exit code.
    pub fn run_raw(
        &self,
        dir: &Path,
        args: &[&str],
        options: RunOptions,
    ) -> Result<RunOutput, GitError> {
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
        for (key, value) in self.env {
            command.env(key, value);
        }
        for (key, value) in &options.env {
            command.env(key, value);
        }

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
    GitError::new(
        classify(&output.stdout_text(), &output.stderr).unwrap_or(ErrorKind::Other),
        text,
    )
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
    if has("nothing to commit")
        || has("no changes added to commit")
        || has("nothing added to commit")
    {
        return Some(ErrorKind::NothingToCommit);
    }
    None
}
