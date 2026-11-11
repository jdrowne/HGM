mod container;
mod fake;
mod image;

use std::sync::atomic::{AtomicUsize, Ordering};

use rustyhgm_core::log::Log;
use rustyhgm_sandbox::{Container, Docker};

pub const BASE_IMAGE: &str = "busybox:latest";

pub fn docker() -> Docker {
    let docker = Docker::new("docker");
    docker
        .available()
        .then_some(docker)
        .expect("the integration tier needs a running Docker daemon")
}

pub fn unique(prefix: &str) -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    format!(
        "{prefix}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    )
}

pub struct Running(pub Container);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.remove();
    }
}

pub fn start() -> Running {
    let name = unique("rustyhgm-sandbox-test");
    let c = docker()
        .run_detached_with(
            BASE_IMAGE,
            &name,
            &["tail", "-f", "/dev/null"],
            &Log::discard(),
        )
        .unwrap();
    Running(c)
}

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_sandbox::CRATE_NAME, "rustyhgm-sandbox");
}
