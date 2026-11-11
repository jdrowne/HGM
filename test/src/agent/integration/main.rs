#[test]
fn crate_is_linked() {
    assert_eq!(hgm_agent::CRATE_NAME, "hgm-agent");
}

fn agent() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_hgm-agent"))
}

#[test]
fn the_agent_binary_reports_its_version() {
    let out = agent().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("hgm-agent ")
    );
}

#[test]
fn the_agent_binary_without_a_known_command_exits_with_usage_status() {
    let out = agent().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .starts_with("hgm-agent: ")
    );
}
