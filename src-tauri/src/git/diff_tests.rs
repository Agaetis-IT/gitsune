//! Tests of the text diff and of commit details, on invented content.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::commit_detail::{ChangeStatus, MAX_AUTO_BLOB_BYTES, commit_detail, file_diff};
use super::open_repository;
use super::tests::{git, merged_repo};
use super::text_diff::{CONTEXT, LineKind, is_binary, line_stats, text_diff};

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

fn replace_line(text: &str, number: usize, with: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| format!("{}\n", if i + 1 == number { with } else { line }))
        .collect()
}

// ---------- text diff ----------

#[test]
fn identical_texts_have_no_hunks() {
    let text = numbered(10);
    let diff = text_diff(text.as_bytes(), text.as_bytes(), 1000);
    assert!(diff.hunks.is_empty());
    assert_eq!(
        (diff.additions, diff.deletions, diff.truncated),
        (0, 0, false)
    );
}

#[test]
fn one_change_in_the_middle_keeps_three_lines_of_context() {
    let old = numbered(10);
    let new = replace_line(&old, 5, "line five, edited");
    let diff = text_diff(old.as_bytes(), new.as_bytes(), 1000);

    assert_eq!(diff.hunks.len(), 1);
    let hunk = &diff.hunks[0];
    assert_eq!(hunk.header, "@@ -2,7 +2,7 @@");
    assert_eq!((diff.additions, diff.deletions), (1, 1));
    let shape: Vec<_> = hunk
        .lines
        .iter()
        .map(|l| (l.kind, l.old_no, l.new_no))
        .collect();
    assert_eq!(shape[0], (LineKind::Context, Some(2), Some(2)));
    assert_eq!(shape[3], (LineKind::Delete, Some(5), None));
    assert_eq!(shape[4], (LineKind::Add, None, Some(5)));
    assert_eq!(shape[7], (LineKind::Context, Some(8), Some(8)));
    assert_eq!(hunk.lines[3].text, "line 5");
    assert_eq!(hunk.lines[4].text, "line five, edited");
}

#[test]
fn close_changes_share_a_hunk_and_distant_ones_do_not() {
    let old = numbered(40);
    // Lines 5 and 12 are 6 unchanged lines apart: one hunk. Line 35 is far: another one.
    let new = replace_line(&replace_line(&replace_line(&old, 5, "a"), 12, "b"), 35, "c");
    let diff = text_diff(old.as_bytes(), new.as_bytes(), 1000);
    assert_eq!(diff.hunks.len(), 2);
    assert_eq!(diff.hunks[0].header, "@@ -2,14 +2,14 @@");
    assert_eq!(diff.hunks[1].header, "@@ -32,7 +32,7 @@");

    // One more line in between and they split.
    let new = replace_line(&replace_line(&old, 5, "a"), 13, "b");
    assert_eq!(
        text_diff(old.as_bytes(), new.as_bytes(), 1000).hunks.len(),
        2
    );
    assert_eq!(CONTEXT, 3);
}

#[test]
fn additions_and_deletions_at_the_edges_and_of_whole_files() {
    let old = numbered(3);
    let appended = format!("{old}line 4\n");
    let diff = text_diff(old.as_bytes(), appended.as_bytes(), 1000);
    assert_eq!(diff.hunks[0].header, "@@ -1,3 +1,4 @@");
    assert_eq!(diff.hunks[0].lines.last().unwrap().kind, LineKind::Add);

    let created = text_diff(b"", old.as_bytes(), 1000);
    assert_eq!(created.hunks[0].header, "@@ -0,0 +1,3 @@");
    assert!(
        created.hunks[0]
            .lines
            .iter()
            .all(|l| l.kind == LineKind::Add)
    );

    let removed = text_diff(old.as_bytes(), b"", 1000);
    assert_eq!(removed.hunks[0].header, "@@ -1,3 +0,0 @@");
    assert_eq!((removed.additions, removed.deletions), (0, 3));
}

#[test]
fn missing_final_newline_and_crlf_are_handled() {
    let diff = text_diff(b"one\r\ntwo\r\n", b"one\r\ntwo\r\nthree", 1000);
    let lines = &diff.hunks[0].lines;
    let last = lines.last().unwrap();
    assert_eq!(
        (last.kind, last.text.as_str(), last.no_eol),
        (LineKind::Add, "three", true)
    );
    assert_eq!(
        lines[0].text, "one",
        "the carriage return is not part of the text"
    );
    assert!(!lines[0].no_eol);
}

#[test]
fn long_diffs_are_cut_but_still_counted() {
    let diff = text_diff(b"", numbered(100).as_bytes(), 10);
    assert!(diff.truncated);
    assert_eq!(diff.hunks[0].lines.len(), 10);
    assert_eq!(diff.hunks[0].new_lines, 10);
    assert_eq!(diff.additions, 100, "totals cover the whole diff");
}

#[test]
fn stats_match_the_diff_and_binary_is_detected() {
    let old = numbered(10);
    let new = replace_line(&old, 3, "x");
    assert_eq!(line_stats(old.as_bytes(), new.as_bytes()), (1, 1));
    assert!(is_binary(b"abc\0def"));
    assert!(!is_binary("héllo\n".as_bytes()));
    assert!(!is_binary(b""));
}

#[test]
fn invalid_utf8_is_replaced_not_fatal() {
    let diff = text_diff(b"", b"caf\xe9\n", 1000);
    assert_eq!(diff.hunks[0].lines[0].text, "caf\u{fffd}");
}

// ---------- commit details ----------

fn write(dir: &Path, path: &str, content: impl AsRef<[u8]>) {
    let full = dir.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

/// c1: a.txt, b.txt, old.txt (root). c2: edits a.txt, deletes b.txt, adds dir/c.txt, renames old.txt.
fn two_commit_repo() -> (TempDir, String, String) {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    write(dir, "a.txt", numbered(10));
    write(dir, "b.txt", "to be removed\n");
    write(dir, "old.txt", numbered(20));
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "First commit"]);
    let first = rev(dir, "HEAD");

    write(dir, "a.txt", replace_line(&numbered(10), 4, "edited"));
    git(dir, &["rm", "-q", "b.txt"]);
    write(dir, "dir/c.txt", "brand new\n");
    git(dir, &["mv", "old.txt", "renamed.txt"]);
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "commit",
            "-q",
            "-m",
            "Second commit\n\nA longer explanation\nover two lines.",
        ],
    );
    let second = rev(dir, "HEAD");
    (tmp, first, second)
}

fn rev(dir: &Path, name: &str) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", name])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn lists_added_modified_deleted_and_renamed_files_with_counts() {
    let (tmp, _, second) = two_commit_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let detail = commit_detail(&repo, &second).unwrap();

    let summary: Vec<_> = detail
        .files
        .iter()
        .map(|f| {
            (
                f.path.as_str(),
                f.old_path.as_deref(),
                f.status,
                f.additions,
                f.deletions,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("a.txt", None, ChangeStatus::Modified, Some(1), Some(1)),
            ("b.txt", None, ChangeStatus::Deleted, Some(0), Some(1)),
            ("dir/c.txt", None, ChangeStatus::Added, Some(1), Some(0)),
            (
                "renamed.txt",
                Some("old.txt"),
                ChangeStatus::Renamed,
                Some(0),
                Some(0)
            ),
        ]
    );
    assert!(!detail.files_truncated);
    assert_eq!(detail.id, second);
    assert_eq!(detail.parents.len(), 1);
    assert_eq!(detail.author_name, "Alice Fakename");
    assert_eq!(detail.author_email, "alice@example.invalid");
    assert_eq!(
        detail.message,
        "Second commit\n\nA longer explanation\nover two lines."
    );
}

#[test]
fn a_root_commit_adds_everything() {
    let (tmp, first, _) = two_commit_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let detail = commit_detail(&repo, &first).unwrap();
    assert!(detail.parents.is_empty());
    assert_eq!(detail.files.len(), 3);
    assert!(detail.files.iter().all(|f| f.status == ChangeStatus::Added));
}

#[test]
fn a_merge_commit_is_diffed_against_its_first_parent() {
    let tmp = merged_repo(); // main: first, second; feature/demo: side (empty commits)
    let dir = tmp.path();
    write(dir, "merged.txt", "from the merge\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--amend", "--no-edit"]);
    let (repo, _) = open_repository(dir).unwrap();
    let head = rev(dir, "HEAD");
    let detail = commit_detail(&repo, &head).unwrap();
    assert_eq!(detail.parents.len(), 2);
    let paths: Vec<_> = detail.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["merged.txt"]);
}

#[test]
fn file_diff_returns_hunks_with_the_right_numbers() {
    let (tmp, _, second) = two_commit_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();

    let modified = file_diff(&repo, &second, "a.txt", None, false).unwrap();
    assert_eq!(modified.status, ChangeStatus::Modified);
    assert_eq!((modified.additions, modified.deletions), (1, 1));
    assert_eq!(modified.hunks.len(), 1);
    assert_eq!(modified.hunks[0].header, "@@ -1,7 +1,7 @@");
    assert!(!modified.binary && !modified.too_large && !modified.truncated);

    let added = file_diff(&repo, &second, "dir/c.txt", None, false).unwrap();
    assert_eq!(added.status, ChangeStatus::Added);
    assert_eq!(
        added.hunks[0].header,
        "@@ -0,0 +1 @@".replace(" @@", ",1 @@")
    );
    assert_eq!(added.hunks[0].lines[0].text, "brand new");

    let deleted = file_diff(&repo, &second, "b.txt", None, false).unwrap();
    assert_eq!(deleted.status, ChangeStatus::Deleted);
    assert_eq!(deleted.new_size, None);

    let renamed = file_diff(&repo, &second, "renamed.txt", Some("old.txt"), false).unwrap();
    assert_eq!(renamed.status, ChangeStatus::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
    assert!(
        renamed.hunks.is_empty(),
        "a pure rename has no content change"
    );
}

#[test]
fn binary_files_have_no_hunks() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    write(dir, "image.bin", [0u8, 1, 2, 3, 0, 255]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "binary"]);
    let (repo, _) = open_repository(dir).unwrap();
    let id = rev(dir, "HEAD");

    let detail = commit_detail(&repo, &id).unwrap();
    assert!(detail.files[0].binary);
    assert_eq!(
        (detail.files[0].additions, detail.files[0].deletions),
        (None, None)
    );
    let diff = file_diff(&repo, &id, "image.bin", None, false).unwrap();
    assert!(diff.binary && diff.hunks.is_empty());
    assert_eq!(diff.new_size, Some(6));
}

#[test]
fn large_files_are_skipped_unless_forced_and_huge_ones_never_computed() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    let big_line = "x".repeat(99) + "\n";
    let large = big_line.repeat((MAX_AUTO_BLOB_BYTES as usize / 100) + 100); // just over 1 MiB
    write(dir, "large.txt", &large);
    write(dir, "huge.txt", big_line.repeat(9 * 1024 * 1024 / 100)); // over 8 MiB
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "big files"]);
    let (repo, _) = open_repository(dir).unwrap();
    let id = rev(dir, "HEAD");

    let detail = commit_detail(&repo, &id).unwrap();
    assert!(
        detail.files.iter().all(|f| f.additions.is_none()),
        "no counts for big blobs"
    );

    let skipped = file_diff(&repo, &id, "large.txt", None, false).unwrap();
    assert!(skipped.too_large && skipped.can_force && skipped.hunks.is_empty());
    assert!(skipped.new_size.unwrap() > MAX_AUTO_BLOB_BYTES);

    let forced = file_diff(&repo, &id, "large.txt", None, true).unwrap();
    assert!(!forced.too_large);
    assert!(forced.truncated, "5000 lines at most are sent");
    assert_eq!(forced.hunks[0].lines.len(), 5000);
    assert!(forced.additions > 5000);

    let huge = file_diff(&repo, &id, "huge.txt", None, true).unwrap();
    assert!(huge.too_large && !huge.can_force && huge.hunks.is_empty());
}

#[test]
fn bad_requests_are_reported() {
    let (tmp, _, second) = two_commit_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    assert!(
        commit_detail(&repo, "not-an-id")
            .unwrap_err()
            .contains("bad commit id")
    );
    assert!(commit_detail(&repo, &"0".repeat(40)).is_err());
    let err = file_diff(&repo, &second, "nope.txt", None, false).unwrap_err();
    assert!(err.contains("not changed"), "{err}");
}

#[test]
fn commit_detail_serializes_with_camel_case_keys() {
    let (tmp, _, second) = two_commit_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let json = serde_json::to_value(commit_detail(&repo, &second).unwrap()).unwrap();
    for key in [
        "id",
        "parents",
        "authorName",
        "authorEmail",
        "authorTime",
        "message",
        "files",
        "filesTruncated",
    ] {
        assert!(json.get(key).is_some(), "missing {key}");
    }
    assert_eq!(json["files"][3]["status"], "renamed");
    assert_eq!(json["files"][3]["oldPath"], "old.txt");

    let diff =
        serde_json::to_value(file_diff(&repo, &second, "a.txt", None, false).unwrap()).unwrap();
    for key in [
        "tooLarge",
        "canForce",
        "truncated",
        "oldSize",
        "newSize",
        "additions",
        "hunks",
        "submodule",
    ] {
        assert!(diff.get(key).is_some(), "missing {key}");
    }
    assert_eq!(diff["hunks"][0]["lines"][3]["kind"], "delete");
    assert_eq!(diff["hunks"][0]["lines"][3]["oldNo"], 4);
    assert_eq!(diff["hunks"][0]["oldStart"], 1);
}
