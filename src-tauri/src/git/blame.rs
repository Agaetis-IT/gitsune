//! Who wrote each line of a file, from `git blame --porcelain`.

use std::collections::HashMap;

use serde::Serialize;

use super::cli::{GitError, RunOptions};
use super::ops::{OpCtx, check_paths};

/// Lines returned at most; a longer file is cut with a flag, not refused.
const MAX_LINES: usize = 5000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlameCommit {
    pub id: String,
    pub author: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub summary: String,
    /// The lines are not committed yet (they are in the working tree).
    pub uncommitted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    /// Index into [`Blame::commits`].
    pub commit: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Blame {
    pub commits: Vec<BlameCommit>,
    pub lines: Vec<BlameLine>,
    /// The file has more lines than are returned.
    pub truncated: bool,
}

fn is_sha(word: &str) -> bool {
    word.len() == 40 && word.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reads the output of `git blame --porcelain`.
pub fn parse_porcelain(text: &str) -> Blame {
    let mut commits: Vec<BlameCommit> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut lines: Vec<BlameLine> = Vec::new();
    let mut truncated = false;
    let mut current: Option<usize> = None;

    for line in text.split('\n') {
        if let Some(content) = line.strip_prefix('\t') {
            if let Some(commit) = current.take() {
                if lines.len() >= MAX_LINES {
                    truncated = true;
                    break;
                }
                lines.push(BlameLine {
                    commit,
                    text: content.trim_end_matches('\r').to_string(),
                });
            }
            continue;
        }
        let mut words = line.split(' ');
        let first = words.next().unwrap_or("");
        if is_sha(first) && words.next().is_some_and(|n| n.parse::<u32>().is_ok()) {
            let at = *index.entry(first.to_string()).or_insert_with(|| {
                commits.push(BlameCommit {
                    id: first.to_string(),
                    author: String::new(),
                    time: 0,
                    summary: String::new(),
                    uncommitted: first.bytes().all(|b| b == b'0'),
                });
                commits.len() - 1
            });
            current = Some(at);
            continue;
        }
        // A header line of the commit that the current entry belongs to.
        if let (Some(at), Some((key, value))) = (current, line.split_once(' ')) {
            match key {
                "author" => commits[at].author = value.to_string(),
                "author-time" => commits[at].time = value.parse().unwrap_or(0),
                "summary" => commits[at].summary = value.to_string(),
                _ => {}
            }
        }
    }
    Blame {
        commits,
        lines,
        truncated,
    }
}

/// Blames a file as it is in `commit`, or as it is in the working tree when `commit` is `None`.
pub fn blame_file(ctx: &OpCtx, commit: Option<&str>, path: &str) -> Result<Blame, GitError> {
    check_paths(&[path.to_string()])?;
    let mut args = vec!["blame", "--porcelain"];
    if let Some(commit) = commit {
        if commit.is_empty() || commit.starts_with('-') || commit.chars().any(char::is_control) {
            return Err(GitError::other("The commit is not valid."));
        }
        args.push(commit);
    }
    args.extend(["--", path]);
    let output = ctx
        .runner()
        .run(ctx.workdir, &args, RunOptions::background())?;
    Ok(parse_porcelain(&output.stdout_text()))
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn the_porcelain_output_is_read_with_repeated_commits() {
        let text = format!(
            "{SHA_A} 1 1 2\nauthor Ada Fakename\nauthor-mail <a@example.invalid>\nauthor-time 1700000000\n\
             summary First\nfilename f.txt\n\tone\n{SHA_A} 2 2\n\ttwo\n\
             {SHA_B} 3 3 1\nauthor Bob Fakename\nauthor-time 1700000100\nsummary Second\nfilename f.txt\n\tthree\n"
        );
        let blame = parse_porcelain(&text);
        assert_eq!(blame.commits.len(), 2);
        assert_eq!(blame.lines.len(), 3);
        assert_eq!(
            blame.lines.iter().map(|l| l.commit).collect::<Vec<_>>(),
            vec![0, 0, 1]
        );
        assert_eq!(blame.commits[0].author, "Ada Fakename");
        assert_eq!(blame.commits[1].summary, "Second");
        assert_eq!(blame.commits[1].time, 1_700_000_100);
        assert_eq!(blame.lines[2].text, "three");
        assert!(!blame.truncated);
    }

    #[test]
    fn lines_not_committed_yet_are_marked() {
        let zero = "0".repeat(40);
        let text = format!(
            "{zero} 1 1 1\nauthor Not Committed Yet\nauthor-time 1\nsummary Version of f.txt from f.txt\n\tnew\n"
        );
        let blame = parse_porcelain(&text);
        assert!(blame.commits[0].uncommitted);
    }

    #[test]
    fn a_long_file_is_cut_and_flagged() {
        let mut text = String::new();
        for n in 1..=MAX_LINES + 10 {
            if n == 1 {
                text.push_str(&format!(
                    "{SHA_A} 1 1 {}\nauthor A\nauthor-time 1\nsummary s\n",
                    MAX_LINES + 10
                ));
            } else {
                text.push_str(&format!("{SHA_A} {n} {n}\n"));
            }
            text.push_str(&format!("\tline {n}\n"));
        }
        let blame = parse_porcelain(&text);
        assert_eq!(blame.lines.len(), MAX_LINES);
        assert!(blame.truncated);
    }
}
