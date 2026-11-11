use std::fs;
use std::path::{Path, PathBuf};

use hgm::agent_build::{BUILD_LOG, Build, Builder};
use hgm_sandbox::Docker;

use crate::improve::{IMAGE, image, mode_patch};

fn builder(root: &Path, image_name: &str) -> Builder {
    Builder {
        docker: Docker::new("docker"),
        context: root.join("context"),
        image_name: image_name.to_owned(),
        rebuild: false,
    }
}

fn patch(root: &Path, name: &str, text: &str) -> PathBuf {
    let path = root.join(format!("{name}.diff"));
    fs::write(&path, text).unwrap();
    path
}

fn log(binary: &Path) -> String {
    fs::read_to_string(binary.with_file_name(BUILD_LOG)).unwrap()
}

fn built() -> (tempfile::TempDir, PathBuf) {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let binary = root.join("node/hgm-agent");
    let ok = patch(root, "ok", &mode_patch("ok"));
    builder(root, IMAGE).build(&[ok], &binary).unwrap();
    (tmp, binary)
}

#[test]
fn the_agent_is_built_from_its_patches_and_copied_out() {
    assert!(built().1.is_file());
}

#[test]
fn the_build_is_logged() {
    assert!(log(&built().1).contains("Compiling the agent"));
}

struct Failed {
    _tmp: tempfile::TempDir,
    binary: PathBuf,
    result: Result<(), String>,
}

fn failed(name: &str, text: &str) -> Failed {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let binary = root.join(name).join("hgm-agent");
    let patches = [patch(root, name, text)];
    let result = builder(root, IMAGE).build(&patches, &binary);
    Failed {
        _tmp: tmp,
        binary,
        result,
    }
}

fn build_failure() -> Failed {
    failed("buildfail", &mode_patch("buildfail"))
}

fn bad_patch() -> Failed {
    failed("badpatch", "--- a/none\n+++ b/none\n@@ -1 +1 @@\n-x\n+y\n")
}

#[test]
fn a_failing_compile_is_an_error() {
    assert!(build_failure().result.is_err());
}

#[test]
fn a_failing_compile_is_logged() {
    assert!(log(&build_failure().binary).contains("Failed to build the agent: "));
}

#[test]
fn a_failing_compile_leaves_no_agent() {
    assert!(!build_failure().binary.exists());
}

#[test]
fn a_patch_that_does_not_apply_is_an_error() {
    assert!(bad_patch().result.is_err());
}

#[test]
fn a_patch_that_does_not_apply_is_logged() {
    assert!(log(&bad_patch().binary).contains("Failed to build the agent: "));
}

#[test]
fn a_patch_that_does_not_apply_leaves_no_agent() {
    assert!(!bad_patch().binary.exists());
}

fn unusable(
    prepare: impl FnOnce(&Path),
    binary: &str,
) -> (tempfile::TempDir, PathBuf, Result<(), String>) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    prepare(root);
    let binary = root.join(binary);
    let result = builder(root, "hgm-no-such-image:never").build(&[], &binary);
    (tmp, binary, result)
}

#[test]
fn a_node_under_a_file_fails_the_build() {
    let (_tmp, _, result) = unusable(
        |root| fs::write(root.join("file"), "").unwrap(),
        "file/hgm-agent",
    );
    assert!(result.is_err());
}

#[test]
fn an_unwritable_build_log_fails_the_build() {
    let (_tmp, _, result) = unusable(
        |root| fs::create_dir_all(root.join("logdir").join(BUILD_LOG)).unwrap(),
        "logdir/hgm-agent",
    );
    assert!(result.is_err());
}

#[test]
fn a_missing_image_fails_the_build() {
    assert!(unusable(|_| {}, "noimage/hgm-agent").2.is_err());
}

#[test]
fn a_missing_image_leaves_no_agent() {
    assert!(!unusable(|_| {}, "noimage/hgm-agent").1.exists());
}
