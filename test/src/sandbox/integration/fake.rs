use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use hgm_core::log::Log;
use hgm_sandbox::{Docker, Error, ExecOptions};

fn fake(body: &str) -> (tempfile::TempDir, Docker) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let docker = Docker::new(path.to_string_lossy());
    (dir, docker)
}

#[test]
fn an_exec_killed_by_a_signal_is_a_command_error() {
    let (_dir, docker) = fake("kill -9 $$");
    let r = docker
        .container("c")
        .exec(&["true"], &ExecOptions::default());
    match r {
        Err(e @ Error::Command { code: None, .. }) => {
            assert!(e.to_string().contains("failed with a signal"), "{e}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_container_that_will_not_be_removed_is_an_error() {
    let (_dir, docker) = fake(r#"case "$1" in container) exit 0;; *) echo refused; exit 1;; esac"#);
    let r = docker.remove_existing_container("c", &Log::discard());
    assert!(
        matches!(r, Err(Error::Command { code: Some(1), .. })),
        "{r:?}"
    );
}

#[test]
fn a_failed_docker_cp_is_an_error_both_ways() {
    let (dir, docker) = fake(r#"case "$1" in exec) exit 0;; *) exit 1;; esac"#);
    let c = docker.container("c");
    let log = Log::discard();
    let src = dir.path().join("docker");
    assert!(matches!(
        c.copy_in(&src, "/x", &log),
        Err(Error::Command { .. })
    ));
    let dest = dir.path().join("out/f");
    assert!(matches!(
        c.copy_out("/x", &dest, &log),
        Err(Error::Command { .. })
    ));
    assert!(dest.parent().is_some_and(Path::is_dir));
}

#[test]
fn copying_out_to_a_bare_file_name_creates_no_directory() {
    let (_dir, docker) = fake("exit 0");
    let c = docker.container("c");
    c.copy_out("/x", Path::new("hgm-fake-copy-target"), &Log::discard())
        .unwrap();
    assert!(!Path::new("hgm-fake-copy-target").exists());
}
