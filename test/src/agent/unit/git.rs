use std::fs;
use std::path::Path;
use std::process::Command;

use hgm_agent::git::{diff_versus_commit, diff_versus_commit_with};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn repo() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "hgm@example.com"]);
    git(dir, &["config", "user.name", "hgm"]);
    git(dir, &["config", "core.autocrlf", "false"]);
    fs::write(dir.join("tracked.txt"), "one\n").unwrap();
    fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    let commit = git(dir, &["rev-parse", "HEAD"]).trim().to_owned();
    (tmp, commit)
}

#[test]
fn a_clean_repo_has_an_empty_diff() {
    let (tmp, commit) = repo();
    assert_eq!(diff_versus_commit(tmp.path(), &commit).unwrap(), "");
}

#[test]
fn tracked_changes_come_first_then_each_untracked_file() {
    let (tmp, commit) = repo();
    let dir = tmp.path();
    fs::write(dir.join("tracked.txt"), "two\n").unwrap();
    fs::write(dir.join("new.txt"), "fresh\n").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/deep.txt"), "deep\n").unwrap();
    fs::write(dir.join("ignored.txt"), "skip\n").unwrap();
    let before = git(dir, &["status", "--porcelain"]);
    let diff = diff_versus_commit(dir, &commit).unwrap();
    assert_eq!(git(dir, &["status", "--porcelain"]), before);
    let tracked = git(dir, &["diff", &commit]);
    assert!(diff.starts_with(&tracked), "{diff}");
    let rest = &diff[tracked.len()..];
    let new = rest.find("diff --git a/new.txt b/new.txt\n").unwrap();
    let deep = rest
        .find("diff --git a/sub/deep.txt b/sub/deep.txt\n")
        .unwrap();
    assert!(new < deep, "{diff}");
    assert!(
        rest.contains("+++ b/new.txt\n@@ -0,0 +1 @@\n+fresh\n"),
        "{diff}"
    );
    assert!(rest.contains("+deep\n"), "{diff}");
    assert!(!diff.contains("skip"), "{diff}");
}

#[test]
fn a_bad_commit_still_lists_untracked_files() {
    let (tmp, _) = repo();
    fs::write(tmp.path().join("new.txt"), "fresh\n").unwrap();
    let diff = diff_versus_commit(tmp.path(), "no-such-commit").unwrap();
    assert!(
        diff.starts_with("diff --git a/new.txt b/new.txt\n"),
        "{diff}"
    );
}

#[test]
fn a_missing_git_program_is_an_error() {
    let (tmp, commit) = repo();
    assert!(diff_versus_commit_with("hgm-no-such-git", tmp.path(), &commit).is_err());
}

#[cfg(unix)]
#[test]
fn a_directory_that_vanishes_before_the_untracked_diffs_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("git");
    fs::write(
        &fake,
        "#!/bin/sh\n[ \"$3\" = ls-files ] && { echo f; rm -rf \"$2\"; }\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let dir = tmp.path().join("repo");
    fs::create_dir(&dir).unwrap();
    assert!(diff_versus_commit_with(fake.to_str().unwrap(), &dir, "c").is_err());
}
