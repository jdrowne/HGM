mod agent;
mod bash;

#[test]
fn crate_is_linked() {
    assert_eq!(hgm_coding_agent::CRATE_NAME, "hgm-coding-agent");
}
