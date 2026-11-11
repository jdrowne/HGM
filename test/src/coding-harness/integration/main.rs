mod bash;
mod harness;

#[test]
fn crate_is_linked() {
    assert_eq!(
        rustyhgm_coding_harness::CRATE_NAME,
        "rustyhgm-coding-harness"
    );
}
