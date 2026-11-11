//! Integration tests for `hgm-sandbox`.

#[test]
fn crate_is_linked() {
    assert_eq!(hgm_sandbox::CRATE_NAME, "hgm-sandbox");
}
