use std::fs;
use std::path::Path;

use hgm_core::log::Log;
use hgm_sandbox::{Container, Docker, Error, ExecOptions, RunOptions};

fn missing() -> Docker {
    Docker::new("hgm-no-such-docker-program")
}

fn container() -> Container {
    missing().container("c")
}

fn is_io<T>(r: Result<T, Error>) -> bool {
    matches!(r, Err(Error::Io(_)))
}

#[test]
fn a_docker_that_cannot_start_is_not_available() {
    assert!(!missing().available());
}

#[test]
fn image_exists_without_docker_is_an_io_error() {
    assert!(is_io(missing().image_exists("i")));
}

#[test]
fn container_exists_without_docker_is_an_io_error() {
    assert!(is_io(missing().container_exists("c")));
}

#[test]
fn build_without_docker_is_an_io_error() {
    assert!(is_io(missing().build(
        Path::new("."),
        "t",
        true,
        &Log::discard()
    )));
}

#[test]
fn ensure_image_without_docker_is_an_io_error() {
    assert!(is_io(missing().ensure_image(
        Path::new("."),
        "t",
        false,
        &Log::discard()
    )));
}

#[test]
fn remove_image_without_docker_is_an_io_error() {
    assert!(is_io(missing().remove_image("i")));
}

#[test]
fn image_tags_without_docker_is_an_io_error() {
    assert!(is_io(missing().image_tags()));
}

#[test]
fn run_detached_without_docker_is_an_io_error() {
    assert!(is_io(missing().run_detached("i", "c", &Log::discard())));
}

#[test]
fn run_detached_as_without_docker_is_an_io_error() {
    assert!(is_io(missing().run_detached_as(
        "i",
        "c",
        &RunOptions::default(),
        &Log::discard()
    )));
}

#[test]
fn remove_existing_container_without_docker_is_an_io_error() {
    assert!(is_io(
        missing().remove_existing_container("c", &Log::discard())
    ));
}

#[test]
fn a_container_keeps_its_name() {
    assert_eq!(container().name(), "c");
}

#[test]
fn exec_without_docker_is_an_io_error() {
    assert!(is_io(container().exec(&["true"], &ExecOptions::default())));
}

#[test]
fn read_file_without_docker_is_an_io_error() {
    assert!(is_io(container().read_file("/f")));
}

#[test]
fn copy_out_without_docker_is_an_io_error() {
    assert!(is_io(container().copy_out(
        "/f",
        Path::new("out"),
        &Log::discard()
    )));
}

#[test]
fn copy_in_without_docker_is_an_io_error() {
    assert!(is_io(container().copy_in(
        Path::new("Cargo.toml"),
        "/x",
        &Log::discard()
    )));
}

#[test]
fn stop_without_docker_is_an_io_error() {
    assert!(is_io(container().stop()));
}

#[test]
fn remove_without_docker_is_an_io_error() {
    assert!(is_io(container().remove()));
}

#[test]
fn cleanup_without_docker_logs_the_failed_removal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docker.log");
    container().cleanup(&Log::to_file(&path).unwrap());
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("Failed to remove container c")
    );
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
