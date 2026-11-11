mod common;
mod harness_build;
mod improve;

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm::FAILED, "failed");
}
