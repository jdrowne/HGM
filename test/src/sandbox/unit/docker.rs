use std::path::Path;

use hgm_core::log::Log;
use hgm_sandbox::{Docker, Error, ExecOptions, RunOptions};

fn missing() -> Docker {
    Docker::new("hgm-no-such-docker-program")
}

fn is_io(r: Result<impl std::fmt::Debug, Error>) {
    assert!(matches!(r, Err(Error::Io(_))), "{r:?}");
}

#[test]
fn a_docker_that_cannot_start_reports_io_errors() {
    let docker = missing();
    let log = Log::discard();
    assert!(!docker.available());
    is_io(docker.image_exists("i"));
    is_io(docker.container_exists("c"));
    is_io(docker.build(Path::new("."), "t", true, &log));
    is_io(docker.ensure_image(Path::new("."), "t", false, &log));
    is_io(docker.remove_image("i"));
    is_io(docker.image_tags());
    is_io(docker.run_detached("i", "c", &log));
    is_io(docker.run_detached_as("i", "c", &RunOptions::default(), &log));
    is_io(docker.remove_existing_container("c", &log));
    let c = docker.container("c");
    assert_eq!(c.name(), "c");
    is_io(c.exec(&["true"], &ExecOptions::default()));
    is_io(c.read_file("/f"));
    is_io(c.copy_out("/f", Path::new("out"), &log));
    is_io(c.copy_in(Path::new("Cargo.toml"), "/x", &log));
    is_io(c.stop());
    is_io(c.remove());
    c.cleanup(&log);
}

#[test]
fn copying_in_a_missing_source_is_not_found() {
    let c = missing().container("c");
    let r = c.copy_in(Path::new("hgm-no-such-source"), "/x", &Log::discard());
    assert!(matches!(r, Err(Error::NotFound(_))), "{r:?}");
}

#[test]
fn unbalanced_quotes_are_a_quoting_error() {
    let r = missing()
        .container("c")
        .exec_str("echo 'a", &ExecOptions::default());
    assert!(matches!(r, Err(Error::Quoting(_))), "{r:?}");
}
