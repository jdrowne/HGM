//! Integration tests for `hgm-bench`.

#[test]
fn crate_is_linked() {
    assert_eq!(hgm_bench::CRATE_NAME, "hgm-bench");
}
