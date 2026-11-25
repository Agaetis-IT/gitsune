//! Line diff of two text blobs, grouped into hunks with context (like `git diff -U3`).

use std::ops::Range;

use gix::diff::blob::{Algorithm, Diff, InternedInput};
use serde::Serialize;

/// Unchanged lines kept around each change.
pub const CONTEXT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    /// 1-based line number in the old file (`None` for added lines).
    pub old_no: Option<u32>,
    /// 1-based line number in the new file (`None` for deleted lines).
    pub new_no: Option<u32>,
    /// The line without its line terminator.
    pub text: String,
    /// The file ends here without a final newline.
    pub no_eol: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// `@@ -a,b +c,d @@`
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDiff {
    pub hunks: Vec<Hunk>,
    pub additions: u32,
    pub deletions: u32,
    /// Some lines were left out because the diff is longer than the limit.
    pub truncated: bool,
}

/// Git treats a blob as binary when there is a NUL byte near its start.
pub fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(8000)].contains(&0)
}

/// Number of added and removed lines, without building the hunks.
pub fn line_stats(old: &[u8], new: &[u8]) -> (u32, u32) {
    let input = InternedInput::new(old, new);
    let diff = Diff::compute(Algorithm::Histogram, &input);
    (diff.count_additions(), diff.count_removals())
}

/// Diffs two texts. At most `max_lines` diff lines are produced.
pub fn text_diff(old: &[u8], new: &[u8], max_lines: usize) -> TextDiff {
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);

    let old_lines: Vec<&[u8]> = old.split_inclusive(|&b| b == b'\n').collect();
    let new_lines: Vec<&[u8]> = new.split_inclusive(|&b| b == b'\n').collect();
    let changes: Vec<(Range<u32>, Range<u32>)> =
        diff.hunks().map(|hunk| (hunk.before, hunk.after)).collect();

    // Changes closer than twice the context share one hunk.
    let mut groups: Vec<&[(Range<u32>, Range<u32>)]> = Vec::new();
    let mut start = 0;
    for i in 1..=changes.len() {
        if i == changes.len() || changes[i].0.start - changes[i - 1].0.end > 2 * CONTEXT {
            groups.push(&changes[start..i]);
            start = i;
        }
    }
    if changes.is_empty() {
        groups.clear();
    }

    let mut hunks = Vec::new();
    let mut emitted = 0usize;
    let mut truncated = false;
    'groups: for group in groups {
        let first = &group[0];
        let last = &group[group.len() - 1];
        let lead = first.0.start.min(CONTEXT);
        let trail = (old_lines.len() as u32 - last.0.end).min(CONTEXT);
        let (mut old_at, mut new_at) = (first.0.start - lead, first.1.start - lead);
        let (old_start, new_start) = (old_at, new_at);

        let mut lines = Vec::new();
        let mut push = |kind: LineKind, old_idx: Option<u32>, new_idx: Option<u32>, raw: &[u8]| {
            if emitted >= max_lines {
                truncated = true;
                return false;
            }
            emitted += 1;
            lines.push(make_line(kind, old_idx, new_idx, raw));
            true
        };

        let mut stop = false;
        for (before, after) in group {
            while old_at < before.start && !stop {
                stop = !push(
                    LineKind::Context,
                    Some(old_at + 1),
                    Some(new_at + 1),
                    old_lines[old_at as usize],
                );
                old_at += 1;
                new_at += 1;
            }
            for idx in before.clone() {
                if stop {
                    break;
                }
                stop = !push(
                    LineKind::Delete,
                    Some(idx + 1),
                    None,
                    old_lines[idx as usize],
                );
            }
            for idx in after.clone() {
                if stop {
                    break;
                }
                stop = !push(LineKind::Add, None, Some(idx + 1), new_lines[idx as usize]);
            }
            old_at = before.end;
            new_at = after.end;
        }
        for _ in 0..trail {
            if stop {
                break;
            }
            stop = !push(
                LineKind::Context,
                Some(old_at + 1),
                Some(new_at + 1),
                old_lines[old_at as usize],
            );
            old_at += 1;
            new_at += 1;
        }

        // Counts come from the finished lines so a truncated hunk stays consistent.
        let old_count = lines.iter().filter(|l| l.old_no.is_some()).count() as u32;
        let new_count = lines.iter().filter(|l| l.new_no.is_some()).count() as u32;
        hunks.push(Hunk {
            old_start: if old_count == 0 {
                old_start
            } else {
                old_start + 1
            },
            old_lines: old_count,
            new_start: if new_count == 0 {
                new_start
            } else {
                new_start + 1
            },
            new_lines: new_count,
            header: String::new(),
            lines,
        });
        let hunk = hunks.last_mut().expect("just pushed");
        hunk.header = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
        );
        if stop {
            break 'groups;
        }
    }

    TextDiff {
        hunks,
        additions: diff.count_additions(),
        deletions: diff.count_removals(),
        truncated,
    }
}

fn make_line(kind: LineKind, old_no: Option<u32>, new_no: Option<u32>, raw: &[u8]) -> DiffLine {
    let no_eol = !raw.ends_with(b"\n");
    let text = raw.strip_suffix(b"\n").unwrap_or(raw);
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    DiffLine {
        kind,
        old_no,
        new_no,
        text: String::from_utf8_lossy(text).into_owned(),
        no_eol,
    }
}
