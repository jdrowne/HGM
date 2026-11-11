//! End-to-end tests of the built `hgm` binary.

use std::process::Command;

#[test]
fn hgm_binary_runs() {
    let out = Command::new(env!("CARGO_BIN_EXE_hgm"))
        .arg("--version")
        .output()
        .expect("run hgm");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("hgm "), "{stdout}");
}
