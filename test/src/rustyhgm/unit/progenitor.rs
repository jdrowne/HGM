use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustyhgm::progenitor;
use rustyhgm_sandbox::source::copy_filtered;

fn repo_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..")
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(dir).unwrap().to_string_lossy();
                files.insert(rel.replace('\\', "/"), fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn tracked() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files", "-z"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .map(str::to_owned)
        .collect()
}

fn sources() -> (BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>) {
    let tmp = tempfile::tempdir().unwrap();
    let built = tmp.path().join("built");
    progenitor::write(&built).unwrap();
    copy_filtered(&built, &tmp.path().join("from_built")).unwrap();
    copy_filtered(&repo_root(), &tmp.path().join("from_repo")).unwrap();
    let tracked = tracked();
    let mut repo = tree(&tmp.path().join("from_repo"));
    repo.retain(|path, _| tracked.contains(path));
    (tree(&tmp.path().join("from_built")), repo)
}

#[test]
fn the_built_in_progenitor_matches_the_one_in_the_repository() {
    let (built, repo) = sources();
    let differing: Vec<&String> = built
        .keys()
        .chain(repo.keys())
        .filter(|path| built.get(*path) != repo.get(*path))
        .collect();
    assert_eq!(differing, Vec::<&String>::new());
}

#[test]
fn the_built_in_progenitor_carries_the_builder_files() {
    let tmp = tempfile::tempdir().unwrap();
    progenitor::write(tmp.path()).unwrap();
    let read = |root: &Path| {
        [".dockerignore", "Dockerfile"].map(|name| fs::read(root.join(name)).unwrap())
    };
    assert_eq!(read(tmp.path()), read(&repo_root()));
}

#[test]
fn the_built_in_progenitor_holds_no_controller_or_grader() {
    let kept: Vec<String> = progenitor::files()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| {
            !path.starts_with("src/coding-agent/") && !path.starts_with("test/src/coding-agent/")
        })
        .collect();
    assert_eq!(
        kept,
        [
            "Cargo.toml",
            "rust-toolchain.toml",
            "LICENSE.txt",
            "Dockerfile",
            ".dockerignore"
        ]
    );
}

#[test]
fn the_progenitor_cannot_be_written_under_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    assert!(progenitor::write(&file.join("progenitor")).is_err());
}

#[test]
fn the_progenitor_cannot_be_written_over_a_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (rel, _) = progenitor::files().remove(0);
    fs::create_dir_all(tmp.path().join(rel)).unwrap();
    assert!(progenitor::write(tmp.path()).is_err());
}
