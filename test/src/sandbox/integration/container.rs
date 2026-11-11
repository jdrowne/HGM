use std::fs;
use std::path::PathBuf;

use hgm_core::log::Log;
use hgm_sandbox::{Error, ExecOptions, ExecResult, RunOptions};
use tempfile::TempDir;

use crate::{BASE_IMAGE, Running, docker, start, unique};

fn interleaved() -> ExecResult {
    start()
        .0
        .sh(
            "echo out; sleep 0.3; echo err >&2; sleep 0.3; echo again; exit 3",
            &ExecOptions::default(),
        )
        .unwrap()
}

#[test]
fn exec_reports_the_exit_code() {
    assert_eq!(interleaved().exit_code, 3);
}

#[test]
fn exec_interleaves_stdout_and_stderr() {
    assert_eq!(interleaved().text(), "out\nerr\nagain\n");
}

fn in_workdir() -> ExecResult {
    let opts = ExecOptions::workdir("/tmp").env("HGM_X", "a b");
    start().0.sh("pwd; echo \"$HGM_X\"", &opts).unwrap()
}

#[test]
fn exec_with_a_workdir_and_env_succeeds() {
    assert!(in_workdir().success());
}

#[test]
fn exec_honours_workdir_and_env() {
    assert_eq!(in_workdir().text(), "/tmp\na b\n");
}

#[test]
fn exec_str_splits_like_a_shell() {
    let r = start()
        .0
        .exec_str(
            "/bin/sh -c 'echo \"$0\" $1' first 'second word'",
            &ExecOptions::default(),
        )
        .unwrap();
    assert_eq!(r.text(), "first second word\n");
}

fn exit_two() -> ExecResult {
    start().0.sh("exit 2", &ExecOptions::default()).unwrap()
}

#[test]
fn log_ignores_a_failure_when_not_asked() {
    assert!(exit_two().log(&Log::discard(), false).is_ok());
}

#[test]
fn log_raises_on_failure_when_asked() {
    assert!(matches!(
        exit_two().log(&Log::discard(), true),
        Err(Error::ExitCode(2))
    ));
}

fn copied_in() -> (Running, TempDir) {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("patch.txt");
    fs::write(&src, "line\n").unwrap();
    c.0.copy_in(&src, "/hgm/deep/parent_patch.txt", &Log::discard())
        .unwrap();
    (c, dir)
}

#[test]
fn a_file_copied_in_lands_at_its_path() {
    let (c, _dir) = copied_in();
    assert_eq!(
        c.0.read_file("/hgm/deep/parent_patch.txt").unwrap(),
        b"line\n"
    );
}

#[test]
fn a_file_copied_out_lands_in_new_folders() {
    let (c, dir) = copied_in();
    let back = dir.path().join("out/nested/back.txt");
    c.0.copy_out("/hgm/deep/parent_patch.txt", &back, &Log::discard())
        .unwrap();
    assert_eq!(fs::read_to_string(back).unwrap(), "line\n");
}

fn tools_copied_in() -> (Running, TempDir) {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let tools = dir.path().join("tools");
    fs::create_dir_all(tools.join("sub")).unwrap();
    fs::write(tools.join("a.py"), "a").unwrap();
    fs::write(tools.join("sub/b.py"), "b").unwrap();
    c.0.copy_in(&tools, "/hgm/tools/", &Log::discard()).unwrap();
    (c, dir)
}

#[test]
fn a_directory_copied_in_lands_at_the_destination() {
    let (c, _dir) = tools_copied_in();
    assert_eq!(c.0.read_file("/hgm/tools/a.py").unwrap(), b"a");
}

#[test]
fn a_directory_copied_in_keeps_its_subfolders() {
    let (c, _dir) = tools_copied_in();
    assert_eq!(c.0.read_file("/hgm/tools/sub/b.py").unwrap(), b"b");
}

#[test]
fn a_directory_copied_out_keeps_its_subfolders() {
    let (c, dir) = tools_copied_in();
    let back = dir.path().join("back");
    c.0.copy_out("/hgm/tools", &back, &Log::discard()).unwrap();
    assert_eq!(fs::read_to_string(back.join("sub/b.py")).unwrap(), "b");
}

fn nowhere() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    (dir, missing)
}

#[test]
fn copying_in_a_missing_path_fails() {
    let (_dir, missing) = nowhere();
    assert!(matches!(
        start().0.copy_in(&missing, "/x", &Log::discard()),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn copying_out_a_missing_path_fails() {
    let (_dir, missing) = nowhere();
    assert!(matches!(
        start().0.copy_out("/nope", &missing, &Log::discard()),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn reading_a_missing_file_fails() {
    assert!(matches!(
        start().0.read_file("/nope"),
        Err(Error::NotFound(_))
    ));
}

fn docker_log() -> (TempDir, Log) {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::to_file(&dir.path().join("docker.log")).unwrap();
    (dir, log)
}

fn log_text(dir: &TempDir) -> String {
    fs::read_to_string(dir.path().join("docker.log")).unwrap()
}

#[test]
fn a_started_container_exists() {
    let c = start();
    assert!(docker().container_exists(c.0.name()).unwrap());
}

#[test]
fn removing_an_existing_container_removes_it() {
    let d = docker();
    let (_dir, log) = docker_log();
    let c = start();
    d.remove_existing_container(c.0.name(), &log).unwrap();
    assert!(!d.container_exists(c.0.name()).unwrap());
}

fn cleaned_up(log: &Log) -> String {
    let c = start();
    c.0.cleanup(log);
    c.0.name().to_owned()
}

#[test]
fn cleanup_removes_the_container() {
    let d = docker();
    let (_dir, log) = docker_log();
    let name = cleaned_up(&log);
    assert!(!d.container_exists(&name).unwrap());
}

#[test]
fn removing_a_missing_container_logs_that_none_was_found() {
    let (dir, log) = docker_log();
    let missing = unique("hgm-sandbox-missing");
    docker().remove_existing_container(&missing, &log).unwrap();
    assert!(log_text(&dir).contains(&format!(
        " - INFO - No existing container with name {missing} found."
    )));
}

#[test]
fn cleanup_logs_the_removal() {
    let (dir, log) = docker_log();
    let name = cleaned_up(&log);
    assert!(log_text(&dir).contains(&format!("Container '{name}' removed.")));
}

#[test]
fn unbalanced_quotes_are_rejected_before_running() {
    assert!(matches!(
        start().0.exec_str("echo 'open", &ExecOptions::default()),
        Err(Error::Quoting(_))
    ));
}

#[test]
fn exec_in_a_missing_container_fails() {
    let d = docker();
    let gone = d.container(&unique("hgm-sandbox-gone"));
    let r = gone.exec(&["true"], &ExecOptions::default()).unwrap();
    assert!(!r.success());
}

#[test]
fn run_detached_as_applies_the_user_platform_and_cpus() {
    let docker = docker();
    let out = std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"])
        .output()
        .unwrap();
    let platform = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let opts = RunOptions {
        user: Some("1234".into()),
        platform: Some(platform),
        nano_cpus: Some(500_000_000),
        read_only_mounts: Vec::new(),
        command: vec!["tail".into(), "-f".into(), "/dev/null".into()],
    };
    let c = Running(
        docker
            .run_detached_as(
                BASE_IMAGE,
                &unique("hgm-sandbox-as"),
                &opts,
                &Log::discard(),
            )
            .unwrap(),
    );
    assert_eq!(
        c.0.sh("id -u", &ExecOptions::default()).unwrap().text(),
        "1234\n"
    );
}

fn mounted() -> (tempfile::TempDir, Running) {
    let tmp = tempfile::tempdir().unwrap();
    let key = tmp.path().join("key");
    std::fs::write(&key, "secret\n").unwrap();
    let opts = RunOptions {
        read_only_mounts: vec![(key, "/run/key".into())],
        command: vec!["tail".into(), "-f".into(), "/dev/null".into()],
        ..RunOptions::default()
    };
    let c = docker()
        .run_detached_as(
            BASE_IMAGE,
            &unique("hgm-sandbox-mount"),
            &opts,
            &Log::discard(),
        )
        .unwrap();
    (tmp, Running(c))
}

#[test]
fn a_read_only_mount_is_readable_in_the_container() {
    let (_tmp, c) = mounted();
    assert_eq!(
        c.0.sh("cat /run/key", &ExecOptions::default())
            .unwrap()
            .text(),
        "secret\n"
    );
}

#[test]
fn a_read_only_mount_cannot_be_written_in_the_container() {
    let (_tmp, c) = mounted();
    assert!(
        !c.0.sh("echo x > /run/key", &ExecOptions::default())
            .unwrap()
            .success()
    );
}

#[test]
fn run_detached_as_with_a_missing_image_is_a_command_error() {
    let bad = docker().run_detached_as(
        "hgm-no-such-image:never",
        &unique("hgm-bad"),
        &RunOptions::default(),
        &Log::discard(),
    );
    assert!(matches!(bad, Err(Error::Command { .. })), "{bad:?}");
}

#[test]
fn files_copied_to_the_root_or_a_relative_path_land_beside_slash() {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("f.txt");
    fs::write(&src, "x").unwrap();
    let log = Log::discard();
    c.0.copy_in(&src, "/", &log).unwrap();
    c.0.copy_in(&src, "/top.txt", &log).unwrap();
    c.0.copy_in(&src, "rel.txt", &log).unwrap();
    let read: Vec<Vec<u8>> = ["/f.txt", "/top.txt", "/rel.txt"]
        .iter()
        .map(|path| c.0.read_file(path).unwrap())
        .collect();
    assert_eq!(read, vec![b"x".to_vec(); 3]);
}

#[test]
fn copying_out_under_a_file_is_an_io_error() {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file"), "").unwrap();
    let r = c.0.copy_out(
        "/etc/hostname",
        &dir.path().join("file/sub/out"),
        &Log::discard(),
    );
    assert!(matches!(r, Err(Error::Io(_))), "{r:?}");
}
