mod agent_build;
mod common;
mod improve;

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm::FAILED, "failed");
}
