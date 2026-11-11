use std::path::PathBuf;
use std::process::Command;

fn binary(name: &str) -> PathBuf {
    std::env::var_os(format!("NEXTEST_BIN_EXE_{name}"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_BIN_EXE_hgm"))
                .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        })
}

#[test]
fn hgm_binary_runs() {
    let out = Command::new(binary("hgm"))
        .arg("--version")
        .output()
        .expect("run hgm");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("hgm "), "{stdout}");
}

#[test]
fn hgm_without_a_known_command_exits_with_usage_status() {
    let out = Command::new(binary("hgm")).output().expect("run hgm");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr).unwrap().starts_with("hgm: "));
}
