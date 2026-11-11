use std::fs;
use std::path::{Path, PathBuf};

use hgm::agent_build::{BUILD_LOG, Build, Builder};
use hgm_sandbox::Docker;

use crate::improve::{IMAGE, image, mode_patch};

fn builder(root: &Path, image_name: &str) -> Builder {
    Builder {
        docker: Docker::default(),
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

#[test]
fn the_agent_is_built_from_its_patches_and_copied_out() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let builder = builder(root, IMAGE);
    let binary = root.join("node/hgm-agent");
    let ok = patch(root, "ok", &mode_patch("ok"));
    builder.build(&[ok], &binary).unwrap();
    assert!(binary.is_file());
    assert!(log(&binary).contains("Compiling the agent"));
    for (name, text) in [
        ("buildfail", mode_patch("buildfail")),
        (
            "badpatch",
            "--- a/none\n+++ b/none\n@@ -1 +1 @@\n-x\n+y\n".to_owned(),
        ),
    ] {
        let binary = root.join(name).join("hgm-agent");
        let patches = [patch(root, name, &text)];
        assert!(builder.build(&patches, &binary).is_err(), "{name}");
        assert!(
            log(&binary).contains("Failed to build the agent: "),
            "{name}"
        );
        assert!(!binary.exists(), "{name}");
    }
}

#[test]
fn builds_fail_before_any_container_when_the_node_is_unusable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let builder = builder(root, "hgm-no-such-image:never");
    fs::write(root.join("file"), "").unwrap();
    assert!(builder.build(&[], &root.join("file/hgm-agent")).is_err());
    fs::create_dir_all(root.join("logdir").join(BUILD_LOG)).unwrap();
    assert!(builder.build(&[], &root.join("logdir/hgm-agent")).is_err());
    let binary = root.join("noimage/hgm-agent");
    assert!(builder.build(&[], &binary).is_err());
    assert!(!binary.exists());
}
