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
    /// Identifies this hunk (its position and exact content): staging it again fails if the
    /// file changed since it was shown.
    pub id: String,
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
    diff_with_raw(old, new, max_lines).0
}

/// The diff, plus for every hunk its lines as raw bytes (terminators included), which are
/// needed to build a patch that git applies exactly.
type RawHunk<'a> = Vec<(LineKind, &'a [u8])>;

fn diff_with_raw<'a>(
    old: &'a [u8],
    new: &'a [u8],
    max_lines: usize,
) -> (TextDiff, Vec<RawHunk<'a>>) {
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
    let mut raws: Vec<RawHunk> = Vec::new();
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
        let mut raw_lines: RawHunk = Vec::new();
        let mut push =
            |kind: LineKind, old_idx: Option<u32>, new_idx: Option<u32>, raw: &'a [u8]| {
                if emitted >= max_lines {
                    truncated = true;
                    return false;
                }
                emitted += 1;
                lines.push(make_line(kind, old_idx, new_idx, raw));
                raw_lines.push((kind, raw));
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
            id: String::new(),
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
        hunk.id = hunk_id(hunk, &raw_lines);
        raws.push(raw_lines);
        if stop {
            break 'groups;
        }
    }

    let result = TextDiff {
        hunks,
        additions: diff.count_additions(),
        deletions: diff.count_removals(),
        truncated,
    };
    (result, raws)
}

/// FNV-1a over the position and the exact bytes of a hunk.
fn hunk_id(hunk: &Hunk, raw: &RawHunk) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for number in [
        hunk.old_start,
        hunk.old_lines,
        hunk.new_start,
        hunk.new_lines,
    ] {
        feed(&number.to_le_bytes());
    }
    for (kind, bytes) in raw {
        feed(&[match kind {
            LineKind::Context => b' ',
            LineKind::Add => b'+',
            LineKind::Delete => b'-',
        }]);
        feed(&(bytes.len() as u64).to_le_bytes());
        feed(bytes);
    }
    format!("{hash:016x}")
}

/// A path as it must be written in the header of a patch: plain when it can be, otherwise
/// C-quoted the way git does (spaces, quotes, backslashes, control and non-ASCII bytes).
fn patch_path(prefix: &str, path: &str) -> Vec<u8> {
    let full = format!("{prefix}/{path}");
    if full
        .bytes()
        .all(|b| b > b' ' && b < 127 && b != b'"' && b != b'\\')
    {
        return full.into_bytes();
    }
    let mut out = vec![b'"'];
    for byte in full.bytes() {
        match byte {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0..=31 | 127..=255 => out.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
            _ => out.push(byte),
        }
    }
    out.push(b'"');
    out
}

/// The first three lines of a patch for one file.
fn patch_header(path: &str) -> Vec<u8> {
    let mut patch = Vec::new();
    let a = patch_path("a", path);
    let b = patch_path("b", path);
    for (prefix, name) in [
        (&b"diff --git "[..], None),
        (&b"--- "[..], Some(&a)),
        (&b"+++ "[..], Some(&b)),
    ] {
        patch.extend_from_slice(prefix);
        match name {
            Some(name) => patch.extend_from_slice(name),
            None => {
                patch.extend_from_slice(&a);
                patch.push(b' ');
                patch.extend_from_slice(&b);
            }
        }
        patch.push(b'\n');
    }
    patch
}

/// The patch of one hunk of the change from `old` to `new`, ready for `git apply`.
///
/// `None` when no hunk has that id, which means the files are not what was displayed.
pub fn hunk_patch(path: &str, old: &[u8], new: &[u8], hunk_id_wanted: &str) -> Option<Vec<u8>> {
    let (diff, raws) = diff_with_raw(old, new, usize::MAX);
    let at = diff.hunks.iter().position(|h| h.id == hunk_id_wanted)?;
    let hunk = &diff.hunks[at];

    let mut patch = patch_header(path);
    patch.extend_from_slice(hunk.header.as_bytes());
    patch.push(b'\n');
    for (kind, bytes) in &raws[at] {
        patch.push(match kind {
            LineKind::Context => b' ',
            LineKind::Add => b'+',
            LineKind::Delete => b'-',
        });
        patch.extend_from_slice(bytes);
        if !bytes.ends_with(b"\n") {
            patch.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
    Some(patch)
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

/// Which way a selection of lines is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMode {
    /// Add the selected changes to the index: the patch is applied as it is.
    Stage,
    /// Take the selected changes out of the index: the patch is applied in reverse.
    Unstage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinePatchError {
    /// No hunk has that id: the files are not what was displayed.
    Stale,
    /// Nothing selected, or a line that is not a change.
    BadSelection,
    /// A line of the hunk has no line ending, which a partial patch cannot express safely.
    NoEol,
}

/// The patch of some lines of one hunk of the change from `old` to `new`. `selected` are indices
/// into the lines of the hunk. A change that is not selected is made to leave the file as it
/// is: for [`LineMode::Stage`] a deleted line stays (as context) and an added one is left out,
/// for [`LineMode::Unstage`], which is applied in reverse, the other way round.
pub fn line_patch(
    path: &str,
    old: &[u8],
    new: &[u8],
    hunk_id_wanted: &str,
    selected: &[usize],
    mode: LineMode,
) -> Result<Vec<u8>, LinePatchError> {
    let (diff, raws) = diff_with_raw(old, new, usize::MAX);
    let at = diff
        .hunks
        .iter()
        .position(|h| h.id == hunk_id_wanted)
        .ok_or(LinePatchError::Stale)?;
    let hunk = &diff.hunks[at];
    let raw = &raws[at];
    if selected.is_empty()
        || selected.iter().any(|&i| {
            raw.get(i)
                .is_none_or(|(kind, _)| *kind == LineKind::Context)
        })
    {
        return Err(LinePatchError::BadSelection);
    }
    if raw.iter().any(|(_, bytes)| !bytes.ends_with(b"\n")) {
        return Err(LinePatchError::NoEol);
    }

    let (mut old_count, mut new_count) = (0u32, 0u32);
    let mut body = Vec::new();
    for (index, (kind, bytes)) in raw.iter().enumerate() {
        let chosen = selected.contains(&index);
        let sign = match (kind, chosen, mode) {
            (LineKind::Context, _, _) => b' ',
            (LineKind::Add, true, _) => b'+',
            (LineKind::Delete, true, _) => b'-',
            // Left alone: the line stays in the file the patch is applied to.
            (LineKind::Delete, false, LineMode::Stage) => b' ',
            (LineKind::Add, false, LineMode::Unstage) => b' ',
            // Left alone: the line is not in the file the patch is applied to.
            (LineKind::Add, false, LineMode::Stage) => continue,
            (LineKind::Delete, false, LineMode::Unstage) => continue,
        };
        if sign != b'+' {
            old_count += 1;
        }
        if sign != b'-' {
            new_count += 1;
        }
        body.push(sign);
        body.extend_from_slice(bytes);
    }

    let mut patch = patch_header(path);
    patch.extend_from_slice(
        format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, old_count, hunk.new_start, new_count
        )
        .as_bytes(),
    );
    patch.extend_from_slice(&body);
    Ok(patch)
}
