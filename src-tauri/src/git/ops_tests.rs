//! Staging and the working tree diff, on throwaway repositories with invented content.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::cli::ErrorKind;
use super::commit_detail::ChangeStatus;
use super::ops::{
    Area, OpCtx, stage_hunk, stage_lines, stage_paths, unstage_hunk, unstage_lines, unstage_paths,
    working_diff,
};
use super::status::FileState;
use super::status_tests::{init_repo, write};
use super::store::RepoStores;
use super::tests::git;
use super::text_diff::LineKind;

pub(super) fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

pub(super) fn replace_line(text: &str, number: usize, with: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| format!("{}\n", if i + 1 == number { with } else { line }))
        .collect()
}

/// Output of a git command in `dir`, for assertions.
pub(super) fn git_out(dir: &Path, args: &[&str]) -> Vec<u8> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
        .stdout
}

fn text_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_out(dir, args)).into_owned()
}

/// A repository with one commit holding `files`.
pub(super) fn repo_with(files: &[(&str, &str)]) -> TempDir {
    let tmp = init_repo();
    for (path, content) in files {
        write(tmp.path(), path, content);
    }
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-q", "-m", "first"]);
    tmp
}

fn ctx<'a>(tmp: &'a TempDir, stores: &'a RepoStores) -> OpCtx<'a> {
    OpCtx {
        workdir: tmp.path(),
        stores,
    }
}

fn paths(items: &[&str]) -> Vec<String> {
    items.iter().map(|p| p.to_string()).collect()
}

fn states(entries: &[super::status::StatusEntry]) -> Vec<(String, FileState)> {
    entries.iter().map(|e| (e.path.clone(), e.state)).collect()
}

// ---------- whole files ----------

#[test]
fn stages_and_unstages_files_of_every_kind() {
    let tmp = repo_with(&[("edit.txt", "one\n"), ("drop.txt", "drop\n")]);
    let dir = tmp.path();
    write(dir, "edit.txt", "one\ntwo\n");
    write(dir, "new.txt", "new\n");
    fs::remove_file(dir.join("drop.txt")).unwrap();
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let status = stage_paths(&ctx, &paths(&["edit.txt", "new.txt", "drop.txt"])).unwrap();
    assert_eq!(
        states(&status.staged),
        vec![
            ("drop.txt".into(), FileState::Deleted),
            ("edit.txt".into(), FileState::Modified),
            ("new.txt".into(), FileState::Added),
        ]
    );
    assert!(status.unstaged.is_empty());

    let status = unstage_paths(&ctx, &paths(&["edit.txt", "new.txt", "drop.txt"])).unwrap();
    assert!(status.staged.is_empty());
    assert_eq!(
        states(&status.unstaged),
        vec![
            ("drop.txt".into(), FileState::Deleted),
            ("edit.txt".into(), FileState::Modified),
            ("new.txt".into(), FileState::Untracked),
        ]
    );
    assert_eq!(
        fs::read_to_string(dir.join("edit.txt")).unwrap(),
        "one\ntwo\n",
        "files keep their content"
    );
}

#[test]
fn the_commands_are_logged_but_the_status_is_not() {
    let tmp = repo_with(&[("a.txt", "a\n")]);
    write(tmp.path(), "a.txt", "changed\n");
    let stores = RepoStores::default();
    stage_paths(&ctx(&tmp, &stores), &paths(&["a.txt"])).unwrap();

    let log = stores.activity.list();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].command, "git add -A -- a.txt");
    assert!(log[0].ok);
}

#[test]
fn unstaging_works_before_the_first_commit() {
    let tmp = init_repo();
    write(tmp.path(), "a.txt", "one\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let status = stage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    assert_eq!(
        states(&status.staged),
        vec![("a.txt".into(), FileState::Added)]
    );

    let status = unstage_paths(&ctx, &paths(&["a.txt"])).unwrap();
    assert!(status.staged.is_empty());
    assert_eq!(
        states(&status.unstaged),
        vec![("a.txt".into(), FileState::Untracked)]
    );
}

#[test]
fn staging_nothing_is_fine_and_bad_paths_are_refused() {
    let tmp = repo_with(&[("a.txt", "a\n")]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    assert!(stage_paths(&ctx, &[]).unwrap().staged.is_empty());
    assert!(stage_paths(&ctx, &paths(&[""])).is_err());
    assert!(stage_paths(&ctx, &paths(&["a\0b"])).is_err());
    assert!(stores.activity.list().is_empty(), "nothing was run");
}

#[test]
fn many_files_are_staged_in_chunks() {
    let tmp = init_repo();
    let names: Vec<String> = (0..1000).map(|i| format!("dir/file-{i}.txt")).collect();
    for name in &names {
        write(tmp.path(), name, "x\n");
    }
    let stores = RepoStores::default();
    let status = stage_paths(&ctx(&tmp, &stores), &names).unwrap();
    assert_eq!(status.staged.len(), 1000);
    assert_eq!(stores.activity.list().len(), 3, "400 + 400 + 200 paths");
}

// ---------- hunks ----------

fn unstaged_hunks(ctx: &OpCtx, path: &str) -> Vec<String> {
    working_diff(ctx, path, None, Area::Unstaged, false)
        .unwrap()
        .hunks
        .into_iter()
        .map(|h| h.id)
        .collect()
}

#[test]
fn stages_one_hunk_and_leaves_the_other_in_the_working_tree() {
    let original = numbered(40);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    let edited = replace_line(
        &replace_line(&original, 3, "line 3, edited"),
        35,
        "line 35, edited",
    );
    write(dir, "f.txt", &edited);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let ids = unstaged_hunks(&ctx, "f.txt");
    assert_eq!(ids.len(), 2);
    let status = stage_hunk(&ctx, "f.txt", &ids[1]).unwrap();

    // Both lists show the file; the index only has the second change.
    assert_eq!(
        states(&status.staged),
        vec![("f.txt".into(), FileState::Modified)]
    );
    assert_eq!(
        states(&status.unstaged),
        vec![("f.txt".into(), FileState::Modified)]
    );
    let staged = text_out(dir, &["diff", "--cached", "-U0"]);
    assert!(
        staged.contains("+line 35, edited") && !staged.contains("line 3, edited"),
        "{staged}"
    );
    let unstaged = text_out(dir, &["diff", "-U0"]);
    assert!(
        unstaged.contains("+line 3, edited") && !unstaged.contains("line 35, edited"),
        "{unstaged}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("f.txt")).unwrap(),
        edited,
        "the working tree is untouched"
    );

    // The staged side now offers that hunk for unstaging.
    let staged_hunks = working_diff(&ctx, "f.txt", None, Area::Staged, false)
        .unwrap()
        .hunks;
    assert_eq!(staged_hunks.len(), 1);
    let status = unstage_hunk(&ctx, "f.txt", &staged_hunks[0].id).unwrap();
    assert!(status.staged.is_empty());
    assert_eq!(unstaged_hunks(&ctx, "f.txt").len(), 2);
}

#[test]
fn a_hunk_that_no_longer_matches_is_refused_and_nothing_changes() {
    let original = numbered(20);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    write(dir, "f.txt", replace_line(&original, 5, "five"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = unstaged_hunks(&ctx, "f.txt");

    // The file changes after the diff was shown.
    write(
        dir,
        "f.txt",
        replace_line(&original, 5, "five, edited again"),
    );
    let error = stage_hunk(&ctx, "f.txt", &ids[0]).unwrap_err();
    assert_eq!(error.kind, ErrorKind::StaleHunk);
    assert!(ctx.status().unwrap().staged.is_empty());
}

#[test]
fn crlf_files_keep_their_line_endings_in_the_index() {
    let crlf = |text: &str| text.replace('\n', "\r\n");
    let original = crlf(&numbered(30));
    let tmp = repo_with(&[("win.txt", &original)]);
    let dir = tmp.path();
    git(dir, &["config", "core.autocrlf", "false"]);
    let edited = crlf(&replace_line(
        &replace_line(&numbered(30), 2, "two!"),
        28,
        "twenty-eight!",
    ));
    write(dir, "win.txt", &edited);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let ids = unstaged_hunks(&ctx, "win.txt");
    stage_hunk(&ctx, "win.txt", &ids[0]).unwrap();
    let index = git_out(dir, &["show", ":0:win.txt"]);
    let expected = crlf(&replace_line(&numbered(30), 2, "two!"));
    assert_eq!(String::from_utf8(index).unwrap(), expected);
}

#[test]
fn a_file_without_a_final_newline_can_be_staged_by_hunk() {
    let tmp = repo_with(&[("end.txt", "first\nsecond")]);
    let dir = tmp.path();
    write(dir, "end.txt", "first\nsecond\nthird");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = unstaged_hunks(&ctx, "end.txt");
    let status = stage_hunk(&ctx, "end.txt", &ids[0]).unwrap();
    assert!(
        status.unstaged.is_empty(),
        "everything was in that one hunk"
    );
    assert_eq!(
        git_out(dir, &["show", ":0:end.txt"]),
        b"first\nsecond\nthird"
    );
}

#[test]
fn bytes_that_are_not_utf8_survive_staging() {
    let mut original = numbered(30).into_bytes();
    original.extend_from_slice(b"caf\xe9\n"); // latin-1
    let mut edited = replace_line(&numbered(30), 2, "two!").into_bytes();
    edited.extend_from_slice(b"caf\xe9\n");
    let tmp = init_repo();
    write(tmp.path(), "latin.txt", &original);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-q", "-m", "first"]);
    write(tmp.path(), "latin.txt", &edited);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let ids = unstaged_hunks(&ctx, "latin.txt");
    stage_hunk(&ctx, "latin.txt", &ids[0]).unwrap();
    assert_eq!(git_out(tmp.path(), &["show", ":0:latin.txt"]), edited);
}

#[test]
fn paths_with_spaces_quotes_and_accents_work() {
    let name = "dir with space/é \"q\".txt";
    let original = numbered(30);
    let tmp = repo_with(&[(name, &original)]);
    write(tmp.path(), name, replace_line(&original, 15, "middle"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let ids = unstaged_hunks(&ctx, name);
    let status = stage_hunk(&ctx, name, &ids[0]).unwrap();
    assert_eq!(status.staged[0].path, name);
    assert!(status.unstaged.is_empty());
}

#[test]
fn hunks_of_untracked_deleted_and_binary_files_are_refused_with_a_reason() {
    let tmp = repo_with(&[("tracked.txt", "a\n"), ("img.bin", "x")]);
    let dir = tmp.path();
    write(dir, "untracked.txt", "new\n");
    fs::remove_file(dir.join("tracked.txt")).unwrap();
    write(dir, "img.bin", [0u8, 1, 2, 0]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let untracked = stage_hunk(&ctx, "untracked.txt", "0000").unwrap_err();
    assert!(untracked.message.contains("not tracked"), "{untracked}");
    let deleted = stage_hunk(&ctx, "tracked.txt", "0000").unwrap_err();
    assert!(deleted.message.contains("no longer exists"), "{deleted}");
    let binary = stage_hunk(&ctx, "img.bin", "0000").unwrap_err();
    assert!(binary.message.contains("Binary"), "{binary}");
    // Whole files can still be staged.
    assert_eq!(
        stage_paths(&ctx, &paths(&["img.bin"]))
            .unwrap()
            .staged
            .len(),
        1
    );
}

// ---------- working tree diffs ----------

#[test]
fn diffs_each_side_of_the_index() {
    let tmp = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
    let dir = tmp.path();
    write(dir, "a.txt", "one\nTWO\nthree\n");
    git(dir, &["add", "a.txt"]);
    write(dir, "a.txt", "one\nTWO\nthree\nfour\n");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let staged = working_diff(&ctx, "a.txt", None, Area::Staged, false).unwrap();
    assert_eq!(
        (staged.additions, staged.deletions, staged.status),
        (1, 1, ChangeStatus::Modified)
    );
    assert_eq!(
        staged.hunks[0]
            .lines
            .iter()
            .find(|l| l.new_no == Some(2))
            .unwrap()
            .text,
        "TWO"
    );

    let unstaged = working_diff(&ctx, "a.txt", None, Area::Unstaged, false).unwrap();
    assert_eq!((unstaged.additions, unstaged.deletions), (1, 0));
    assert_eq!(unstaged.hunks[0].lines.last().unwrap().text, "four");
}

#[test]
fn diffs_new_deleted_and_renamed_files() {
    let tmp = repo_with(&[("gone.txt", "bye\n"), ("old.txt", &numbered(20))]);
    let dir = tmp.path();
    write(dir, "fresh.txt", "hello\n");
    fs::remove_file(dir.join("gone.txt")).unwrap();
    git(dir, &["mv", "old.txt", "renamed.txt"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let fresh = working_diff(&ctx, "fresh.txt", None, Area::Unstaged, false).unwrap();
    assert_eq!(fresh.status, ChangeStatus::Added);
    assert_eq!(fresh.hunks[0].header, "@@ -0,0 +1,1 @@");

    let gone = working_diff(&ctx, "gone.txt", None, Area::Unstaged, false).unwrap();
    assert_eq!((gone.status, gone.deletions), (ChangeStatus::Deleted, 1));

    let moved = working_diff(&ctx, "renamed.txt", Some("old.txt"), Area::Staged, false).unwrap();
    assert_eq!(moved.status, ChangeStatus::Renamed);
    assert!(
        moved.hunks.is_empty(),
        "a pure rename has no content change"
    );
}

#[test]
fn big_and_binary_working_files_are_handled_like_commit_files() {
    let tmp = init_repo();
    let dir = tmp.path();
    write(dir, "bin.dat", [0u8, 255, 0, 1]);
    write(dir, "big.txt", "x".repeat(99).clone().to_string() + "\n");
    let big = ("y".repeat(99) + "\n").repeat(11_000); // just over 1 MiB
    write(dir, "big.txt", &big);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    assert!(
        working_diff(&ctx, "bin.dat", None, Area::Unstaged, false)
            .unwrap()
            .binary
    );
    let skipped = working_diff(&ctx, "big.txt", None, Area::Unstaged, false).unwrap();
    assert!(skipped.too_large && skipped.can_force && skipped.hunks.is_empty());
    let forced = working_diff(&ctx, "big.txt", None, Area::Unstaged, true).unwrap();
    assert!(!forced.too_large && !forced.hunks.is_empty());
}

// ---------- safety ----------

#[test]
fn read_only_mode_refuses_every_write_before_running_git() {
    let tmp = repo_with(&[("a.txt", "a\n")]);
    write(tmp.path(), "a.txt", "changed\n");
    let stores = RepoStores::default();
    stores.set_read_only(true);
    let ctx = ctx(&tmp, &stores);

    assert_eq!(
        stage_paths(&ctx, &paths(&["a.txt"])).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        unstage_paths(&ctx, &paths(&["a.txt"])).unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert_eq!(
        stage_hunk(&ctx, "a.txt", "x").unwrap_err().kind,
        ErrorKind::ReadOnly
    );
    assert!(
        stores.activity.list().is_empty(),
        "no git command was started"
    );
    assert!(ctx.status().unwrap().staged.is_empty());

    stores.set_read_only(false);
    assert_eq!(
        stage_paths(&ctx, &paths(&["a.txt"])).unwrap().staged.len(),
        1
    );
}

#[test]
fn a_lock_held_by_another_process_is_reported_and_never_removed() {
    let tmp = repo_with(&[("a.txt", "a\n")]);
    write(tmp.path(), "a.txt", "changed\n");
    let lock = tmp.path().join(".git/index.lock");
    fs::write(&lock, "").unwrap();
    let stores = RepoStores::default();

    let error = stage_paths(&ctx(&tmp, &stores), &paths(&["a.txt"])).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Locked, "{error}");
    assert!(lock.exists(), "the lock file is never deleted");

    fs::remove_file(&lock).unwrap();
    assert_eq!(
        stage_paths(&ctx(&tmp, &stores), &paths(&["a.txt"]))
            .unwrap()
            .staged
            .len(),
        1
    );
}

#[test]
fn writes_of_the_app_run_one_at_a_time() {
    let tmp = repo_with(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
    write(tmp.path(), "a.txt", "a2\n");
    write(tmp.path(), "b.txt", "b2\n");
    let stores = RepoStores::default();

    std::thread::scope(|scope| {
        for name in ["a.txt", "b.txt"] {
            let (tmp, stores) = (&tmp, &stores);
            scope.spawn(move || {
                let ctx = ctx(tmp, stores);
                for _ in 0..15 {
                    stage_paths(&ctx, &paths(&[name])).unwrap();
                    unstage_paths(&ctx, &paths(&[name])).unwrap();
                }
            });
        }
    });

    let mut log = stores.activity.list();
    assert_eq!(log.len(), 60);
    assert!(
        log.iter().all(|e| e.ok),
        "a lock clash would show as a failed command"
    );
    log.sort_by_key(|e| e.started_at);
    for pair in log.windows(2) {
        assert!(
            pair[0].started_at + pair[0].duration_ms <= pair[1].started_at + 1,
            "commands overlapped: {:?} and {:?}",
            pair[0].command,
            pair[1].command
        );
    }
}

// ---------- single lines ----------

/// Indices, in the first hunk of a file, of the lines of a kind whose text is `text`.
fn line_index(ctx: &OpCtx, path: &str, area: Area, kind: LineKind, text: &str) -> (String, usize) {
    let diff = working_diff(ctx, path, None, area, false).unwrap();
    let hunk = &diff.hunks[0];
    let at = hunk
        .lines
        .iter()
        .position(|l| l.kind == kind && l.text == text)
        .unwrap_or_else(|| panic!("no {kind:?} line {text:?}"));
    (hunk.id.clone(), at)
}

fn index_text(dir: &Path, path: &str) -> String {
    text_out(dir, &["show", &format!(":{path}")])
}

#[test]
fn stages_only_the_added_line_and_keeps_the_removed_one() {
    let original = numbered(10);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    // Line 6 is replaced: one line removed, one added, in the same hunk.
    write(dir, "f.txt", replace_line(&original, 6, "six"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let (id, add) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Add, "six");
    stage_lines(&ctx, "f.txt", &id, &[add]).unwrap();

    let expected = original.replace("line 6\n", "line 6\nsix\n");
    assert_eq!(index_text(dir, "f.txt"), expected);
    assert_eq!(
        fs::read_to_string(dir.join("f.txt")).unwrap(),
        replace_line(&original, 6, "six"),
        "the working tree is untouched"
    );
}

#[test]
fn stages_only_the_removed_line() {
    let original = numbered(10);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    write(dir, "f.txt", replace_line(&original, 6, "six"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let (id, delete) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Delete, "line 6");
    stage_lines(&ctx, "f.txt", &id, &[delete]).unwrap();

    assert_eq!(index_text(dir, "f.txt"), original.replace("line 6\n", ""));
}

#[test]
fn staging_the_two_halves_one_after_the_other_gives_the_whole_change() {
    let original = numbered(10);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    let edited = replace_line(&original, 6, "six");
    write(dir, "f.txt", &edited);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let (id, add) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Add, "six");
    stage_lines(&ctx, "f.txt", &id, &[add]).unwrap();
    let (id, delete) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Delete, "line 6");
    stage_lines(&ctx, "f.txt", &id, &[delete]).unwrap();

    assert_eq!(index_text(dir, "f.txt"), edited);
    assert!(
        text_out(dir, &["diff"]).is_empty(),
        "nothing is left unstaged"
    );
}

#[test]
fn stages_some_lines_among_several_changes() {
    let original = numbered(12);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    // Two added lines and one removed one, close enough to share a hunk.
    let edited = original
        .replace("line 3\n", "line 3\nnew A\n")
        .replace("line 5\n", "")
        .replace("line 7\n", "line 7\nnew B\n");
    write(dir, "f.txt", &edited);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    let (id, b) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Add, "new B");
    let (_, five) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Delete, "line 5");
    stage_lines(&ctx, "f.txt", &id, &[b, five]).unwrap();

    let expected = original
        .replace("line 5\n", "")
        .replace("line 7\n", "line 7\nnew B\n");
    assert_eq!(index_text(dir, "f.txt"), expected);
}

#[test]
fn unstages_only_the_chosen_lines() {
    let original = numbered(10);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    write(dir, "f.txt", replace_line(&original, 6, "six"));
    git(dir, &["add", "f.txt"]);
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);

    // Put the removed line back in the index: the added one stays staged.
    let (id, delete) = line_index(&ctx, "f.txt", Area::Staged, LineKind::Delete, "line 6");
    unstage_lines(&ctx, "f.txt", &id, &[delete]).unwrap();
    assert_eq!(
        index_text(dir, "f.txt"),
        original.replace("line 6\n", "line 6\nsix\n")
    );

    // And now the added one: the index is the committed file again.
    let (id, add) = line_index(&ctx, "f.txt", Area::Staged, LineKind::Add, "six");
    unstage_lines(&ctx, "f.txt", &id, &[add]).unwrap();
    assert_eq!(index_text(dir, "f.txt"), original);
}

#[test]
fn a_bad_selection_or_a_stale_hunk_changes_nothing() {
    let original = numbered(10);
    let tmp = repo_with(&[("f.txt", &original)]);
    let dir = tmp.path();
    write(dir, "f.txt", replace_line(&original, 6, "six"));
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let (id, add) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Add, "six");

    assert!(
        stage_lines(&ctx, "f.txt", &id, &[]).is_err(),
        "nothing selected"
    );
    assert!(
        stage_lines(&ctx, "f.txt", &id, &[0]).is_err(),
        "a context line"
    );
    assert!(
        stage_lines(&ctx, "f.txt", &id, &[999]).is_err(),
        "out of range"
    );
    write(dir, "f.txt", replace_line(&original, 6, "six again"));
    let error = stage_lines(&ctx, "f.txt", &id, &[add]).unwrap_err();
    assert_eq!(error.kind, ErrorKind::StaleHunk);
    assert_eq!(index_text(dir, "f.txt"), original);
}

#[test]
fn a_hunk_with_a_line_without_ending_is_refused_for_lines() {
    let tmp = repo_with(&[("f.txt", "a\nb\nc")]);
    let dir = tmp.path();
    write(dir, "f.txt", "a\nb\nc\nd");
    let stores = RepoStores::default();
    let ctx = ctx(&tmp, &stores);
    let (id, add) = line_index(&ctx, "f.txt", Area::Unstaged, LineKind::Add, "d");
    assert!(stage_lines(&ctx, "f.txt", &id, &[add]).is_err());
}
