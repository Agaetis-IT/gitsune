//! Status parsing (on sample output) and reading (on throwaway repositories).

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::activity::ActivityLog;
use super::cli::Runner;
use super::status::{FileState, WorkingStatus, parse_status, read_status};
use super::tests::git;

fn nul(records: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for record in records {
        out.extend_from_slice(record.as_bytes());
        out.push(0);
    }
    out
}

fn summary(entries: &[super::status::StatusEntry]) -> Vec<(String, FileState)> {
    entries.iter().map(|e| (e.path.clone(), e.state)).collect()
}

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

// ---------- parsing ----------

#[test]
fn parses_the_branch_header() {
    let out = nul(&[
        &format!("# branch.oid {OID}"),
        "# branch.head main",
        "# branch.upstream origin/main",
        "# branch.ab +3 -1",
    ]);
    let status = parse_status(&out);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.head.as_deref(), Some(OID));
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (3, 1));
    assert!(!status.detached);
    assert!(status.staged.is_empty() && status.unstaged.is_empty());
}

#[test]
fn parses_a_detached_head_and_a_repository_without_commits() {
    let detached = parse_status(&nul(&[
        &format!("# branch.oid {OID}"),
        "# branch.head (detached)",
    ]));
    assert!(detached.detached);
    assert_eq!(detached.branch, None);

    let unborn = parse_status(&nul(&["# branch.oid (initial)", "# branch.head main"]));
    assert_eq!(unborn.head, None);
    assert_eq!(unborn.branch.as_deref(), Some("main"));
}

#[test]
fn a_file_changed_in_the_index_and_again_in_the_tree_is_listed_twice() {
    let line = format!("1 MM N... 100644 100644 100644 {OID} {OID} src/a.ts");
    let status = parse_status(&nul(&[&line]));
    assert_eq!(
        summary(&status.staged),
        vec![("src/a.ts".into(), FileState::Modified)]
    );
    assert_eq!(
        summary(&status.unstaged),
        vec![("src/a.ts".into(), FileState::Modified)]
    );
}

#[test]
fn reads_each_kind_of_change_on_each_side() {
    let status = parse_status(&nul(&[
        &format!("1 A. N... 000000 100644 100644 {OID} {OID} added.txt"),
        &format!("1 .D N... 100644 100644 000000 {OID} {OID} gone.txt"),
        &format!("1 T. N... 100644 120000 120000 {OID} {OID} link"),
        &format!("1 .M N... 100644 100644 100644 {OID} {OID} edited.txt"),
    ]));
    assert_eq!(
        summary(&status.staged),
        vec![
            ("added.txt".into(), FileState::Added),
            ("link".into(), FileState::TypeChanged)
        ]
    );
    assert_eq!(
        summary(&status.unstaged),
        vec![
            ("edited.txt".into(), FileState::Modified),
            ("gone.txt".into(), FileState::Deleted)
        ]
    );
}

#[test]
fn a_rename_carries_its_previous_path() {
    let line = format!("2 R. N... 100644 100644 100644 {OID} {OID} R100 new name.txt");
    let status = parse_status(&nul(&[&line, "old name.txt"]));
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, "new name.txt");
    assert_eq!(status.staged[0].old_path.as_deref(), Some("old name.txt"));
    assert_eq!(status.staged[0].state, FileState::Renamed);
    assert!(status.unstaged.is_empty());
}

#[test]
fn paths_with_spaces_and_odd_characters_survive() {
    let line =
        format!("1 .M N... 100644 100644 100644 {OID} {OID} dir with space/é ü \"quoted\".txt");
    let status = parse_status(&nul(&[&line, "? new file.txt"]));
    assert_eq!(status.unstaged[0].path, "dir with space/é ü \"quoted\".txt");
    assert_eq!(status.unstaged[1].path, "new file.txt");
    assert_eq!(status.unstaged[1].state, FileState::Untracked);
}

#[test]
fn conflicts_are_listed_apart() {
    let line = format!("u UU N... 100644 100644 100644 100644 {OID} {OID} {OID} both.txt");
    let status = parse_status(&nul(&[&line]));
    assert_eq!(status.conflicted.len(), 1);
    assert_eq!(
        (
            status.conflicted[0].path.as_str(),
            status.conflicted[0].code.as_str()
        ),
        ("both.txt", "UU")
    );
    assert!(status.staged.is_empty() && status.unstaged.is_empty());
}

#[test]
fn submodules_are_flagged_and_unknown_records_ignored() {
    let line = format!("1 .M S.M. 160000 160000 160000 {OID} {OID} vendor/lib");
    let status = parse_status(&nul(&[&line, "! ignored.log", "weird record"]));
    assert!(status.unstaged[0].submodule);
    assert_eq!(status.unstaged.len(), 1);
}

#[test]
fn empty_output_is_a_clean_status() {
    assert_eq!(parse_status(b""), WorkingStatus::default());
}

// ---------- on real repositories ----------

pub(super) fn init_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Alice Fakename"]);
    git(dir, &["config", "user.email", "alice@example.invalid"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    // Hooks of this repository only, whatever the global configuration says.
    let hooks = dir.join(".git/hooks");
    git(dir, &["config", "core.hooksPath", &hooks.to_string_lossy()]);
    tmp
}

pub(super) fn write(dir: &Path, path: &str, content: impl AsRef<[u8]>) {
    let full = dir.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn status_of(dir: &Path) -> WorkingStatus {
    let log = ActivityLog::default();
    let runner = Runner {
        log: &log,
        env: &[],
    };
    let status = read_status(&runner, dir).unwrap();
    assert!(log.list().is_empty(), "background status is not logged");
    status
}

#[test]
fn a_clean_repository_has_no_changes() {
    let tmp = init_repo();
    write(tmp.path(), "a.txt", "one\n");
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-q", "-m", "first"]);
    let status = status_of(tmp.path());
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.head.is_some());
    assert!(status.staged.is_empty() && status.unstaged.is_empty() && status.conflicted.is_empty());
}

#[test]
fn reads_a_mixed_working_tree() {
    let tmp = init_repo();
    let dir = tmp.path();
    write(dir, "keep.txt", "keep\n");
    write(dir, "edit.txt", "one\ntwo\n");
    write(dir, "drop.txt", "drop\n");
    write(
        dir,
        "move.txt",
        "a long enough line to be detected as the same file\nsecond line\nthird line\n",
    );
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "first"]);

    write(dir, "edit.txt", "one\ntwo changed\n");
    git(dir, &["add", "edit.txt"]);
    write(dir, "edit.txt", "one\ntwo changed\nthree\n"); // staged and unstaged
    fs::remove_file(dir.join("drop.txt")).unwrap(); // deleted, not staged
    git(dir, &["mv", "move.txt", "moved.txt"]); // staged rename
    write(dir, "new.txt", "brand new\n"); // untracked
    write(dir, "added.txt", "added\n");
    git(dir, &["add", "added.txt"]); // staged addition

    let status = status_of(dir);
    assert_eq!(
        summary(&status.staged),
        vec![
            ("added.txt".into(), FileState::Added),
            ("edit.txt".into(), FileState::Modified),
            ("moved.txt".into(), FileState::Renamed),
        ]
    );
    assert_eq!(status.staged[2].old_path.as_deref(), Some("move.txt"));
    assert_eq!(
        summary(&status.unstaged),
        vec![
            ("drop.txt".into(), FileState::Deleted),
            ("edit.txt".into(), FileState::Modified),
            ("new.txt".into(), FileState::Untracked),
        ]
    );
}

#[test]
fn works_before_the_first_commit() {
    let tmp = init_repo();
    write(tmp.path(), "a.txt", "one\n");
    git(tmp.path(), &["add", "a.txt"]);
    write(tmp.path(), "b.txt", "two\n");
    let status = status_of(tmp.path());
    assert_eq!(status.head, None);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(
        summary(&status.staged),
        vec![("a.txt".into(), FileState::Added)]
    );
    assert_eq!(
        summary(&status.unstaged),
        vec![("b.txt".into(), FileState::Untracked)]
    );
}

#[test]
fn reports_a_detached_head_and_an_unresolved_conflict() {
    let tmp = init_repo();
    let dir = tmp.path();
    write(dir, "c.txt", "base\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["checkout", "-q", "-b", "other"]);
    write(dir, "c.txt", "other side\n");
    git(dir, &["commit", "-q", "-am", "other"]);
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "c.txt", "main side\n");
    git(dir, &["commit", "-q", "-am", "main"]);
    let _ = std::process::Command::new("git")
        .current_dir(dir)
        .args(["merge", "other"])
        .output();

    let status = status_of(dir);
    assert_eq!(status.conflicted.len(), 1);
    assert_eq!(status.conflicted[0].path, "c.txt");

    git(dir, &["merge", "--abort"]);
    git(dir, &["checkout", "-q", "--detach", "HEAD"]);
    let detached = status_of(dir);
    assert!(detached.detached && detached.branch.is_none());
}
