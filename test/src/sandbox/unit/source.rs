use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm_core::log::Log;
use rustyhgm_core::pyjson;
use rustyhgm_sandbox::source::{
    CopyReport, copy_filtered, image_name_for, prepare_progenitor, progenitor_metadata,
};
use rustyhgm_sandbox::{Docker, Error};

#[test]
fn progenitor_metadata_matches_json_dump() {
    assert_eq!(
        pyjson::dumps(&progenitor_metadata(), None),
        "{\"run_id\": \"progenitor\", \"overall_performance\": {\"accuracy_score\": 0, \
         \"total_resolved_instances\": 0, \"total_submitted_instances\": 0, \"files\": [], \
         \"total_unresolved_ids\": [], \"total_emptypatch_ids\": [], \"total_resolved_ids\": [], \
         \"total_submitted_ids\": []}}"
    );
}

#[test]
fn image_is_named_after_the_parent_folder() {
    assert_eq!(
        image_name_for(Path::new("/x/progenitor_swe/src")).as_deref(),
        Some("progenitor_swe")
    );
}

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("repo");
    fs::create_dir_all(src.join("pkg/__pycache__")).unwrap();
    fs::create_dir_all(src.join("logs")).unwrap();
    fs::write(
        src.join(".dockerignore"),
        "# c\nlogs/\n**/__pycache__/\n.dockerignore\n",
    )
    .unwrap();
    fs::write(src.join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(src.join("main.py"), "print(1)\n").unwrap();
    fs::write(src.join("pkg/mod.py"), "x = 1\n").unwrap();
    fs::write(src.join("pkg/__pycache__/mod.pyc"), "").unwrap();
    fs::write(src.join("logs/run.log"), "").unwrap();
    dir
}

fn copied() -> (tempfile::TempDir, PathBuf, CopyReport) {
    let dir = tree();
    let dest = dir.path().join("out/progenitor/src");
    let report = copy_filtered(&dir.path().join("repo"), &dest).unwrap();
    (dir, dest, report)
}

fn prepared() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tree();
    let dest = dir.path().join("out/progenitor/src");
    let meta = dir.path().join("out/progenitor/metadata.json");
    prepare_progenitor(&dir.path().join("repo"), &dest, None, &Log::discard()).unwrap();
    (dir, dest, meta)
}

fn broken_dockerignore() -> (tempfile::TempDir, PathBuf) {
    let dir = tree();
    let src = dir.path().join("repo");
    fs::write(src.join(".dockerignore"), "!\n").unwrap();
    (dir, src)
}

#[test]
fn copy_counts_the_dockerignore_patterns() {
    let (_dir, _dest, report) = copied();
    assert_eq!(report.patterns, 3);
}

#[test]
fn copy_skips_ignored_files() {
    let (_dir, _dest, report) = copied();
    assert_eq!(report.copied, ["Dockerfile", "main.py", "pkg/mod.py"]);
}

#[test]
fn copy_keeps_file_contents() {
    let (_dir, dest, _report) = copied();
    assert_eq!(
        fs::read_to_string(dest.join("pkg/mod.py")).unwrap(),
        "x = 1\n"
    );
}

#[test]
fn copy_leaves_ignored_directories_out() {
    let (_dir, dest, _report) = copied();
    assert!(!dest.join("logs").exists());
}

#[test]
fn prepare_writes_the_progenitor_metadata() {
    let (_dir, _dest, meta) = prepared();
    assert_eq!(
        fs::read_to_string(&meta).unwrap(),
        pyjson::dumps(&progenitor_metadata(), None)
    );
}

#[test]
fn prepare_keeps_existing_metadata() {
    let (dir, dest, meta) = prepared();
    fs::write(&meta, "{}").unwrap();
    prepare_progenitor(&dir.path().join("repo"), &dest, None, &Log::discard()).unwrap();
    assert_eq!(fs::read_to_string(&meta).unwrap(), "{}");
}

#[test]
fn copying_below_a_file_is_an_error() {
    let dir = tree();
    let blocker = dir.path().join("file");
    fs::write(&blocker, "").unwrap();
    assert!(copy_filtered(&dir.path().join("repo"), &blocker.join("dest")).is_err());
}

#[test]
fn copying_a_file_onto_a_directory_is_an_error() {
    let dir = tree();
    let clash = dir.path().join("clash");
    fs::create_dir_all(clash.join("main.py")).unwrap();
    assert!(copy_filtered(&dir.path().join("repo"), &clash).is_err());
}

#[test]
fn copying_a_directory_onto_a_file_is_an_error() {
    let dir = tree();
    let clash = dir.path().join("clash2");
    fs::create_dir_all(&clash).unwrap();
    fs::write(clash.join("pkg"), "").unwrap();
    assert!(copy_filtered(&dir.path().join("repo"), &clash).is_err());
}

#[test]
fn copying_with_an_invalid_dockerignore_is_an_error() {
    let (dir, src) = broken_dockerignore();
    assert!(copy_filtered(&src, &dir.path().join("d")).is_err());
}

#[test]
fn preparing_with_an_invalid_dockerignore_is_an_error() {
    let (dir, src) = broken_dockerignore();
    assert!(prepare_progenitor(&src, &dir.path().join("d"), None, &Log::discard()).is_err());
}

#[test]
fn copying_with_an_unreadable_dockerignore_is_an_error() {
    let dir = tree();
    let src = dir.path().join("repo");
    fs::remove_file(src.join(".dockerignore")).unwrap();
    fs::create_dir(src.join(".dockerignore")).unwrap();
    assert!(copy_filtered(&src, &dir.path().join("d")).is_err());
}

#[test]
fn building_needs_a_parent_folder() {
    let dir = tree();
    let docker = Docker::new("rustyhgm-no-such-docker-program");
    let r = prepare_progenitor(
        &dir.path().join("repo"),
        Path::new(""),
        Some(&docker),
        &Log::discard(),
    );
    assert!(matches!(r, Err(Error::NotFound(_))), "{r:?}");
}

#[test]
fn an_empty_path_has_no_image_name() {
    assert_eq!(image_name_for(Path::new("")), None);
}

#[test]
fn the_root_has_no_image_name() {
    assert_eq!(image_name_for(Path::new("/")), None);
}

#[test]
fn building_needs_docker() {
    let dir = tree();
    let docker = Docker::new("rustyhgm-no-such-docker-program");
    let r = prepare_progenitor(
        &dir.path().join("repo"),
        &dir.path().join("out/progenitor/src"),
        Some(&docker),
        &Log::discard(),
    );
    assert!(matches!(r, Err(Error::Io(_))), "{r:?}");
}

#[test]
fn building_needs_a_dockerfile() {
    let dir = tree();
    let src = dir.path().join("repo");
    fs::write(src.join(".dockerignore"), "").unwrap();
    fs::remove_file(src.join("Dockerfile")).unwrap();
    let docker = Docker::new("rustyhgm-no-such-docker-program");
    let r = prepare_progenitor(
        &src,
        &dir.path().join("out/progenitor/src"),
        Some(&docker),
        &Log::discard(),
    );
    assert!(matches!(r, Err(Error::Io(_))), "{r:?}");
}

#[cfg(unix)]
fn dangling() -> (tempfile::TempDir, PathBuf, PathBuf, CopyReport) {
    let dir = tree();
    let src = dir.path().join("repo");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), src.join("dangling")).unwrap();
    let dest = dir.path().join("out/progenitor/src");
    let report = copy_filtered(&src, &dest).unwrap();
    (dir, src, dest, report)
}

#[cfg(unix)]
#[test]
fn dangling_links_are_skipped() {
    let (_dir, _src, _dest, report) = dangling();
    assert!(!report.copied.contains(&"dangling".to_owned()));
}

#[cfg(unix)]
#[test]
fn an_unwritable_metadata_file_is_an_error() {
    let (dir, src, dest, _report) = dangling();
    std::os::unix::fs::symlink(
        dir.path().join("no/dir/m.json"),
        dir.path().join("out/progenitor/metadata.json"),
    )
    .unwrap();
    assert!(prepare_progenitor(&src, &dest, None, &Log::discard()).is_err());
}
