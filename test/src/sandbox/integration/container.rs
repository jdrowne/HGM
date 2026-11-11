use std::fs;

use hgm_core::log::Log;
use hgm_sandbox::{Error, ExecOptions};

use crate::{docker, start, unique};

#[test]
fn exec_interleaves_stdout_and_stderr_and_reports_the_exit_code() {
    let c = start();
    let r =
        c.0.sh(
            "echo out; sleep 0.3; echo err >&2; sleep 0.3; echo again; exit 3",
            &ExecOptions::default(),
        )
        .unwrap();
    assert_eq!(r.exit_code, 3);
    assert_eq!(r.text(), "out\nerr\nagain\n");
}

#[test]
fn exec_honours_workdir_and_env() {
    let c = start();
    let opts = ExecOptions::workdir("/tmp").env("HGM_X", "a b");
    let r = c.0.sh("pwd; echo \"$HGM_X\"", &opts).unwrap();
    assert!(r.success());
    assert_eq!(r.text(), "/tmp\na b\n");
}

#[test]
fn passthrough_env_skips_unset_variables() {
    let opts = ExecOptions::default().passthrough_env(&["PATH", "HGM_SURELY_UNSET_VARIABLE"]);
    assert_eq!(opts.env.len(), 1);
    assert_eq!(opts.env[0].0, "PATH");
}

#[test]
fn exec_str_splits_like_a_shell() {
    let c = start();
    let r =
        c.0.exec_str(
            "/bin/sh -c 'echo \"$0\" $1' first 'second word'",
            &ExecOptions::default(),
        )
        .unwrap();
    assert_eq!(r.text(), "first second word\n");
}

#[test]
fn log_raises_on_failure_only_when_asked() {
    let c = start();
    let r = c.0.sh("exit 2", &ExecOptions::default()).unwrap();
    assert!(r.log(&Log::discard(), false).is_ok());
    assert!(matches!(
        r.log(&Log::discard(), true),
        Err(Error::ExitCode(2))
    ));
}

#[test]
fn copy_file_round_trip() {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("patch.txt");
    fs::write(&src, "line\n").unwrap();
    c.0.copy_in(&src, "/hgm/deep/parent_patch.txt", &Log::discard())
        .unwrap();
    assert_eq!(
        c.0.read_file("/hgm/deep/parent_patch.txt").unwrap(),
        b"line\n"
    );
    let back = dir.path().join("out/nested/back.txt");
    c.0.copy_out("/hgm/deep/parent_patch.txt", &back, &Log::discard())
        .unwrap();
    assert_eq!(fs::read_to_string(back).unwrap(), "line\n");
}

#[test]
fn copy_directory_lands_at_the_destination() {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let tools = dir.path().join("tools");
    fs::create_dir_all(tools.join("sub")).unwrap();
    fs::write(tools.join("a.py"), "a").unwrap();
    fs::write(tools.join("sub/b.py"), "b").unwrap();
    c.0.copy_in(&tools, "/hgm/tools/", &Log::discard()).unwrap();
    assert_eq!(c.0.read_file("/hgm/tools/a.py").unwrap(), b"a");
    assert_eq!(c.0.read_file("/hgm/tools/sub/b.py").unwrap(), b"b");
    let back = dir.path().join("back");
    c.0.copy_out("/hgm/tools", &back, &Log::discard()).unwrap();
    assert_eq!(fs::read_to_string(back.join("sub/b.py")).unwrap(), "b");
}

#[test]
fn copying_missing_paths_fails() {
    let c = start();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    assert!(matches!(
        c.0.copy_in(&missing, "/x", &Log::discard()),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        c.0.copy_out("/nope", &missing, &Log::discard()),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(c.0.read_file("/nope"), Err(Error::NotFound(_))));
}

#[test]
fn cleanup_and_remove_existing() {
    let d = docker();
    let log_dir = tempfile::tempdir().unwrap();
    let log = Log::to_file(&log_dir.path().join("docker.log")).unwrap();
    let missing = unique("hgm-sandbox-missing");
    d.remove_existing_container(&missing, &log).unwrap();
    let c = start();
    assert!(d.container_exists(c.0.name()).unwrap());
    d.remove_existing_container(c.0.name(), &log).unwrap();
    assert!(!d.container_exists(c.0.name()).unwrap());
    let c = start();
    c.0.cleanup(&log);
    assert!(!d.container_exists(c.0.name()).unwrap());
    let text = fs::read_to_string(log_dir.path().join("docker.log")).unwrap();
    assert!(text.contains(&format!(
        " - INFO - No existing container with name {missing} found."
    )));
    assert!(text.contains(&format!("Container '{}' removed.", c.0.name())));
}

#[test]
fn unbalanced_quotes_are_rejected_before_running() {
    let c = start();
    assert!(matches!(
        c.0.exec_str("echo 'open", &ExecOptions::default()),
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
