mod agent;
mod bash;

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_coding_agent::CRATE_NAME, "rustyhgm-coding-agent");
}
