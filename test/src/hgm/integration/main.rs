#[cfg(unix)]
mod agent_build;
#[cfg(unix)]
mod common;
#[cfg(unix)]
mod improve;

#[test]
fn crate_is_linked() {
    assert_eq!(hgm::FAILED, "failed");
}
