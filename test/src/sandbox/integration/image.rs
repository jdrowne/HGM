use std::fs;
use std::path::PathBuf;

use rustyhgm_core::log::Log;
use rustyhgm_sandbox::source::prepare_progenitor;
use rustyhgm_sandbox::{Error, ExecOptions};

use tempfile::TempDir;

use crate::{BASE_IMAGE, Running, docker, unique};

struct Image(String);

impl Drop for Image {
    fn drop(&mut self) {
        let _ = docker().remove_image(&self.0);
    }
}

struct Prepared {
    _dir: TempDir,
    image: Image,
    dest: PathBuf,
    copied: Vec<String>,
}

fn prepared() -> Prepared {
    let d = docker();
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join("logs")).unwrap();
    fs::write(
        repo.join("Dockerfile"),
        format!(
            "FROM {BASE_IMAGE}\nWORKDIR /rustyhgm\nCOPY . .\nCMD [\"tail\", \"-f\", \"/dev/null\"]\n"
        ),
    )
    .unwrap();
    fs::write(repo.join(".dockerignore"), "logs/\n").unwrap();
    fs::write(repo.join("harness.txt"), "v1").unwrap();
    fs::write(repo.join("logs/secret.log"), "no").unwrap();
    let name = unique("rustyhgm-progenitor");
    let dest = dir.path().join(&name).join("src");
    let image = Image(name);
    let report = prepare_progenitor(&repo, &dest, Some(&d), &Log::discard()).unwrap();
    Prepared {
        _dir: dir,
        image,
        dest,
        copied: report.copied,
    }
}

fn rerun(p: &Prepared, rebuild: bool) -> Running {
    let d = docker();
    let log = Log::discard();
    fs::write(p.dest.join("harness.txt"), "v2").unwrap();
    d.ensure_image(&p.dest, &p.image.0, rebuild, &log).unwrap();
    Running(
        d.run_detached(&p.image.0, &unique("rustyhgm-run"), &log)
            .unwrap(),
    )
}

#[test]
fn preparing_the_progenitor_copies_the_unignored_files() {
    assert_eq!(
        prepared().copied,
        [".dockerignore", "Dockerfile", "harness.txt"]
    );
}

#[test]
fn preparing_the_progenitor_builds_its_image() {
    let p = prepared();
    assert!(docker().image_exists(&p.image.0).unwrap());
}

#[test]
fn the_built_image_is_tagged_latest() {
    let p = prepared();
    assert!(
        docker()
            .image_tags()
            .unwrap()
            .contains(&format!("{}:latest", p.image.0))
    );
}

#[test]
fn ensuring_an_existing_image_skips_the_build() {
    let p = prepared();
    let c = rerun(&p, false);
    let r =
        c.0.exec_str("cat harness.txt", &ExecOptions::default())
            .unwrap();
    assert_eq!(r.text(), "v1");
}

#[test]
fn ignored_files_stay_out_of_the_image() {
    let p = prepared();
    let c = rerun(&p, false);
    assert!(
        !c.0.sh("test -e /rustyhgm/logs", &ExecOptions::default())
            .unwrap()
            .success()
    );
}

#[test]
fn ensuring_an_image_with_rebuild_builds_it_again() {
    let p = prepared();
    let c = rerun(&p, true);
    assert_eq!(c.0.read_file("/rustyhgm/harness.txt").unwrap(), b"v2");
}

fn broken() -> (TempDir, String, Error) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Dockerfile"),
        "FROM busybox:latest\nRUN exit 7\n",
    )
    .unwrap();
    let tag = unique("rustyhgm-broken");
    let err = docker()
        .build(dir.path(), &tag, false, &Log::discard())
        .unwrap_err();
    (dir, tag, err)
}

#[test]
fn a_failed_build_is_a_command_error() {
    let (_dir, _tag, err) = broken();
    assert!(matches!(err, Error::Command { .. }), "{err}");
}

#[test]
fn a_failed_build_leaves_no_image() {
    let (_dir, tag, _err) = broken();
    assert!(!docker().image_exists(&tag).unwrap());
}

#[test]
fn running_an_image_that_failed_to_build_is_an_error() {
    let (_dir, tag, _err) = broken();
    assert!(
        docker()
            .run_detached(&tag, &unique("rustyhgm-none"), &Log::discard())
            .is_err()
    );
}
