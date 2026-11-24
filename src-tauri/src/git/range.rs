//! The commits of a branch that its target does not have yet, for the text of a pull request.

use serde::Serialize;

use super::cli::{GitError, RunOptions};
use super::ops::OpCtx;

/// Commits listed at most; a pull request with more is not described commit by commit.
const MAX_COMMITS: usize = 50;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RangeCommit {
    pub id: String,
    pub summary: String,
}

/// The commits reachable from `head` and not from `base`, oldest first.
pub fn commits_between(ctx: &OpCtx, base: &str, head: &str) -> Result<Vec<RangeCommit>, GitError> {
    for name in [base, head] {
        if name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
            return Err(GitError::other("The branch name is not valid."));
        }
    }
    let limit = format!("-n{MAX_COMMITS}");
    let range = format!("{base}..{head}");
    let output = ctx.runner().run(
        ctx.workdir,
        &[
            "log",
            "--reverse",
            "--format=%H%x1f%s",
            &limit,
            &range,
            "--",
        ],
        RunOptions::background(),
    )?;
    Ok(output
        .stdout_text()
        .lines()
        .filter_map(|line| {
            let (id, summary) = line.split_once('\u{1f}')?;
            Some(RangeCommit {
                id: id.to_string(),
                summary: summary.to_string(),
            })
        })
        .collect())
}

/// An author of the repository and how many commits they wrote.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorCount {
    pub name: String,
    pub count: usize,
}

/// Commits read to tell the authors of a very large history.
const MAX_AUTHOR_SCAN: usize = 50_000;
/// Authors listed, the most active first.
const MAX_AUTHORS: usize = 60;

/// The authors of the commits of every branch, the most active first. Names that differ only by
/// case, accents or spacing count as one person (the first spelling seen is kept).
pub fn list_authors(ctx: &OpCtx) -> Result<Vec<AuthorCount>, GitError> {
    let limit = format!("-n{MAX_AUTHOR_SCAN}");
    let output = ctx.runner().run_raw(
        ctx.workdir,
        &["log", "--all", "--format=%an", &limit, "--"],
        RunOptions::background(),
    )?;
    if output.code != 0 {
        // A repository without a commit has no history to read.
        return Ok(Vec::new());
    }
    Ok(count_authors(&output.stdout_text()))
}

fn person_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn count_authors(text: &str) -> Vec<AuthorCount> {
    let mut order: Vec<(String, AuthorCount)> = Vec::new();
    for name in text.lines().map(str::trim).filter(|n| !n.is_empty()) {
        let key = person_key(name);
        match order.iter_mut().find(|(k, _)| *k == key) {
            Some((_, entry)) => entry.count += 1,
            None => order.push((
                key,
                AuthorCount {
                    name: name.to_string(),
                    count: 1,
                },
            )),
        }
    }
    let mut authors: Vec<AuthorCount> = order.into_iter().map(|(_, entry)| entry).collect();
    authors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    authors.truncate(MAX_AUTHORS);
    authors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_are_counted_by_person_and_sorted() {
        let authors = count_authors("Ada Lovelace\nbob\nada  lovelace\n\nBob\nBob\n");
        assert_eq!(
            authors,
            vec![
                AuthorCount {
                    name: "bob".into(),
                    count: 3
                },
                AuthorCount {
                    name: "Ada Lovelace".into(),
                    count: 2
                },
            ]
        );
    }
}
