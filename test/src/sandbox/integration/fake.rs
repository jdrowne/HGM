use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm_core::log::Log;
use rustyhgm_sandbox::{Docker, Error};

#[cfg(unix)]
fn script(dir: &Path, sh: &str, _cmd: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("docker");
    fs::write(&path, format!("#!/bin/sh\n{sh}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(windows)]
fn script(dir: &Path, _sh: &str, cmd: &str) -> PathBuf {
    let path = dir.join("docker.cmd");
    fs::write(&path, format!("@echo off\r\n{cmd}\r\n")).unwrap();
    path
}

fn fake(sh: &str, cmd: &str) -> (tempfile::TempDir, Docker, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = script(dir.path(), sh, cmd);
    let docker = Docker::new(path.to_string_lossy());
    (dir, docker, path)
}

#[cfg(unix)]
#[test]
fn an_exec_killed_by_a_signal_is_a_command_error() {
    let (_dir, docker, _) = fake("kill -9 $$", "");
    let r = docker
        .container("c")
        .exec(&["true"], &rustyhgm_sandbox::ExecOptions::default());
    match r {
        Err(e @ Error::Command { code: None, .. }) => {
            assert!(e.to_string().contains("failed with a signal"), "{e}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_container_that_will_not_be_removed_is_an_error() {
    let (_dir, docker, _) = fake(
        r#"case "$1" in container) exit 0;; *) echo refused; exit 1;; esac"#,
        "if \"%1\"==\"container\" exit /b 0\r\necho refused\r\nexit /b 1",
    );
    let r = docker.remove_existing_container("c", &Log::discard());
    assert!(
        matches!(r, Err(Error::Command { code: Some(1), .. })),
        "{r:?}"
    );
}

fn failing_cp() -> (tempfile::TempDir, Docker, PathBuf) {
    fake(
        r#"case "$1" in exec) exit 0;; *) exit 1;; esac"#,
        "if \"%1\"==\"exec\" exit /b 0\r\nexit /b 1",
    )
}

#[test]
fn a_failed_docker_cp_in_is_a_command_error() {
    let (_dir, docker, src) = failing_cp();
    assert!(matches!(
        docker.container("c").copy_in(&src, "/x", &Log::discard()),
        Err(Error::Command { .. })
    ));
}

#[test]
fn a_failed_docker_cp_out_is_a_command_error() {
    let (dir, docker, _) = failing_cp();
    assert!(matches!(
        docker
            .container("c")
            .copy_out("/x", &dir.path().join("out/f"), &Log::discard()),
        Err(Error::Command { .. })
    ));
}

#[test]
fn a_failed_docker_cp_out_still_creates_the_destination_folder() {
    let (dir, docker, _) = failing_cp();
    let dest = dir.path().join("out/f");
    docker
        .container("c")
        .copy_out("/x", &dest, &Log::discard())
        .unwrap_err();
    assert!(dest.parent().is_some_and(Path::is_dir));
}

#[test]
fn copying_out_to_a_bare_file_name_creates_no_directory() {
    let (_dir, docker, _) = fake("exit 0", "exit /b 0");
    let c = docker.container("c");
    c.copy_out(
        "/x",
        Path::new("rustyhgm-fake-copy-target"),
        &Log::discard(),
    )
    .unwrap();
    assert!(!Path::new("rustyhgm-fake-copy-target").exists());
}
