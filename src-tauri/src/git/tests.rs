//! Tests on tiny throwaway repositories built with the git CLI (invented data only).

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::refs::RefKind;
use super::{list_refs, open_repository};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=Alice Fakename",
            "-c",
            "user.email=alice@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .status()
        .expect("git must be installed");
    assert!(status.success(), "git {args:?} failed");
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}

/// main with two commits, a feature branch, a lightweight tag, an annotated tag
/// and a fake remote-tracking ref.
fn sample_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, "first");
    git(dir, &["branch", "feature/demo"]);
    commit(dir, "second");
    git(dir, &["tag", "v1.0.0"]);
    git(dir, &["tag", "-a", "v2.0.0", "-m", "annotated"]);
    git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    tmp
}

#[test]
fn opens_a_repository_and_reports_its_head_branch() {
    let tmp = sample_repo();
    let (_, info) = open_repository(tmp.path()).unwrap();
    assert_eq!(info.head_branch.as_deref(), Some("main"));
    assert!(!info.is_bare);
    assert_eq!(info.name, tmp.path().file_name().unwrap().to_string_lossy());
}

#[test]
fn opening_a_plain_directory_fails() {
    let tmp = TempDir::new().unwrap();
    let err = open_repository(tmp.path()).unwrap_err();
    assert!(err.contains("cannot open repository"), "{err}");
}

#[test]
fn lists_local_remote_and_tag_refs_sorted_by_kind_then_name() {
    let tmp = sample_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let refs = list_refs(&repo).unwrap();

    let summary: Vec<_> = refs.iter().map(|r| (r.kind, r.name.as_str())).collect();
    assert_eq!(
        summary,
        vec![
            (RefKind::Local, "feature/demo"),
            (RefKind::Local, "main"),
            (RefKind::Remote, "origin/main"),
            (RefKind::Tag, "v1.0.0"),
            (RefKind::Tag, "v2.0.0"),
        ]
    );
}

#[test]
fn marks_the_head_branch_and_peels_annotated_tags_to_commits() {
    let tmp = sample_repo();
    let (repo, _) = open_repository(tmp.path()).unwrap();
    let refs = list_refs(&repo).unwrap();
    let by_name = |n: &str| refs.iter().find(|r| r.name == n).unwrap();

    assert!(by_name("main").is_head);
    assert!(!by_name("feature/demo").is_head);
    // Both tags point at the same commit as main once the annotated one is peeled.
    assert_eq!(by_name("v2.0.0").target, by_name("main").target);
    assert_eq!(by_name("v1.0.0").target, by_name("main").target);
    assert_ne!(by_name("feature/demo").target, by_name("main").target);
}
