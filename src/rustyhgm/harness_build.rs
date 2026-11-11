use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use rustyhgm_bench::swe::{Step, message, run_step};
use rustyhgm_core::log::Log;
use rustyhgm_sandbox::{Container, Docker};

use crate::node::at;

pub const HARNESS_BINARY: &str = "rustyhgm-coding-harness";
pub const INSTALLED_HARNESS: &str = "/usr/local/bin/rustyhgm-coding-harness";
pub const BUILD_LOG: &str = "build_harness.log";

const BUILD: [&str; 3] = [
    "/bin/sh",
    "-c",
    "cargo build --release --target \"$(uname -m)-unknown-linux-musl\" -p rustyhgm-coding-harness && cp \"target/$(uname -m)-unknown-linux-musl/release/rustyhgm-coding-harness\" /usr/local/bin/rustyhgm-coding-harness",
];

pub trait Build: Sync {
    fn build(&self, patches: &[PathBuf], binary: &Path) -> Result<(), String>;
}

pub fn patch_steps(patches: &[PathBuf]) -> Vec<Step> {
    patches
        .iter()
        .flat_map(|patch| {
            [
                Step::CopyIn(patch.clone(), "/rustyhgm/parent_patch.txt"),
                Step::Run(
                    &["/bin/sh", "-c", "patch -p1 < /rustyhgm/parent_patch.txt"],
                    "/rustyhgm",
                ),
                Step::Run(&["rm", "/rustyhgm/parent_patch.txt"], "/rustyhgm"),
            ]
        })
        .collect()
}

pub fn compile(container: &Container, log: &Log) -> Result<(), String> {
    log.info("Compiling the harness");
    run_step(container, &Step::Run(&BUILD, "/rustyhgm"), log).map_err(message)
}

pub fn copy_binary(container: &Container, binary: &Path, log: &Log) -> Result<(), String> {
    container
        .copy_out(INSTALLED_HARNESS, binary, log)
        .map_err(message)
}

pub struct Builder {
    pub docker: Docker,
    pub context: PathBuf,
    pub image_name: String,
    pub rebuild: bool,
}

impl Builder {
    fn steps(
        &self,
        container: &Container,
        patches: &[PathBuf],
        binary: &Path,
        log: &Log,
    ) -> Result<(), String> {
        patch_steps(patches)
            .iter()
            .try_for_each(|step| run_step(container, step, log))
            .map_err(message)
            .and_then(|()| compile(container, log))
            .and_then(|()| copy_binary(container, binary, log))
    }
}

impl Build for Builder {
    fn build(&self, patches: &[PathBuf], binary: &Path) -> Result<(), String> {
        let dir = binary.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|e| at(dir, e))?;
        let log_path = dir.join(BUILD_LOG);
        let log = Log::to_file(&log_path).map_err(|e| at(&log_path, e))?;
        let name = format!(
            "rustyhgm-build-{}",
            Local::now().format("%Y%m%d_%H%M%S_%6f")
        );
        let container = self
            .docker
            .remove_existing_container(&name, &log)
            .and_then(|()| {
                self.docker
                    .ensure_image(&self.context, &self.image_name, self.rebuild, &log)
            })
            .and_then(|()| self.docker.run_detached(&self.image_name, &name, &log))
            .map_err(message)?;
        let built = self.steps(&container, patches, binary, &log);
        container.cleanup(&log);
        built.inspect_err(|e| log.error(format!("Failed to build the harness: {e}")))
    }
}
