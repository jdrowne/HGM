use std::fs;

use hgm_core::log::Log;
use hgm_sandbox::source::prepare_initial_agent;
use hgm_sandbox::{Error, ExecOptions};

use crate::{BASE_IMAGE, docker, unique};

struct Image(String);

impl Drop for Image {
    fn drop(&mut self) {
        let _ = docker().remove_image(&self.0);
    }
}

#[test]
fn build_skip_and_run_the_agent_image() {
    let d = docker();
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join("logs")).unwrap();
    fs::write(
        repo.join("Dockerfile"),
        format!(
            "FROM {BASE_IMAGE}\nWORKDIR /hgm\nCOPY . .\nCMD [\"tail\", \"-f\", \"/dev/null\"]\n"
        ),
    )
    .unwrap();
    fs::write(repo.join(".dockerignore"), "logs/\n").unwrap();
    fs::write(repo.join("agent.txt"), "v1").unwrap();
    fs::write(repo.join("logs/secret.log"), "no").unwrap();

    let name = unique("hgm-initial");
    let dest = dir.path().join(&name).join("src");
    let image = Image(name.clone());
    let log = Log::discard();
    let report = prepare_initial_agent(&repo, &dest, Some(&d), &log).unwrap();
    assert_eq!(report.copied, [".dockerignore", "Dockerfile", "agent.txt"]);
    assert!(d.image_exists(&image.0).unwrap());
    assert!(
        d.image_tags()
            .unwrap()
            .contains(&format!("{}:latest", image.0))
    );

    fs::write(dest.join("agent.txt"), "v2").unwrap();
    d.ensure_image(&dest, &image.0, false, &log).unwrap();
    let c = d.run_detached(&image.0, &unique("hgm-run"), &log).unwrap();
    let r = c
        .exec_str("cat agent.txt", &ExecOptions::default())
        .unwrap();
    assert_eq!(r.text(), "v1");
    assert!(
        !c.sh("test -e /hgm/logs", &ExecOptions::default())
            .unwrap()
            .success()
    );
    c.remove().unwrap();

    d.ensure_image(&dest, &image.0, true, &log).unwrap();
    let c = d.run_detached(&image.0, &unique("hgm-run"), &log).unwrap();
    assert_eq!(c.read_file("/hgm/agent.txt").unwrap(), b"v2");
    c.remove().unwrap();
}

#[test]
fn failed_builds_and_runs_are_errors() {
    let d = docker();
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Dockerfile"),
        "FROM busybox:latest\nRUN exit 7\n",
    )
    .unwrap();
    let tag = unique("hgm-broken");
    let err = d
        .build(dir.path(), &tag, false, &Log::discard())
        .unwrap_err();
    assert!(matches!(err, Error::Command { .. }), "{err}");
    assert!(!d.image_exists(&tag).unwrap());
    assert!(
        d.run_detached(&tag, &unique("hgm-none"), &Log::discard())
            .is_err()
    );
}
