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
    if !out.status.success() {
        panic!("{}", String::from_utf8_lossy(&out.stderr));
    }
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

struct Changed {
    tmp: tempfile::TempDir,
    commit: String,
    before: String,
    after: String,
    diff: String,
}

fn changed() -> Changed {
    let (tmp, commit) = repo();
    let dir = tmp.path();
    fs::write(dir.join("tracked.txt"), "two\n").unwrap();
    fs::write(dir.join("new.txt"), "fresh\n").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/deep.txt"), "deep\n").unwrap();
    fs::write(dir.join("ignored.txt"), "skip\n").unwrap();
    let before = git(dir, &["status", "--porcelain"]);
    let diff = diff_versus_commit(dir, &commit).unwrap();
    let after = git(dir, &["status", "--porcelain"]);
    Changed {
        commit,
        before,
        after,
        diff,
        tmp,
    }
}

impl Changed {
    fn tracked(&self) -> String {
        git(self.tmp.path(), &["diff", &self.commit])
    }

    fn untracked(&self) -> String {
        let tracked = self.tracked();
        self.diff
            .strip_prefix(tracked.as_str())
            .unwrap_or_default()
            .to_owned()
    }
}

#[test]
fn diffing_leaves_the_repo_status_alone() {
    let changed = changed();
    assert_eq!(changed.after, changed.before);
}

#[test]
fn tracked_changes_come_first() {
    let changed = changed();
    assert!(
        changed.diff.starts_with(&changed.tracked()),
        "{}",
        changed.diff
    );
}

#[test]
fn untracked_files_follow_in_path_order() {
    let rest = changed().untracked();
    let new = rest.find("diff --git a/new.txt b/new.txt\n").unwrap();
    let deep = rest
        .find("diff --git a/sub/deep.txt b/sub/deep.txt\n")
        .unwrap();
    assert!(new < deep, "{rest}");
}

#[test]
fn an_untracked_file_is_diffed_as_new() {
    let rest = changed().untracked();
    assert!(
        rest.contains("+++ b/new.txt\n@@ -0,0 +1 @@\n+fresh\n"),
        "{rest}"
    );
}

#[test]
fn untracked_files_in_subdirectories_are_diffed() {
    let rest = changed().untracked();
    assert!(rest.contains("+deep\n"), "{rest}");
}

#[test]
fn ignored_files_are_left_out_of_the_diff() {
    let diff = changed().diff;
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
