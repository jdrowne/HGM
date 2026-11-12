use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm_core::log::Log;
use rustyhgm_core::metadata::NODE_METADATA_FILE;
use rustyhgm_core::pyjson::{self, PyValue};

use crate::docker::Docker;
use crate::error::Error;
use crate::ignore::{DOCKERIGNORE_FILE, Spec, read_dockerignore};

pub const DOCKERFILE: &str = "Dockerfile";

#[derive(Debug, Default)]
pub struct CopyReport {
    pub patterns: usize,
    pub copied: Vec<String>,
}

pub fn progenitor_metadata() -> PyValue {
    PyValue::dict([
        ("run_id", PyValue::str("progenitor")),
        (
            "overall_performance",
            PyValue::dict([
                ("accuracy_score", PyValue::Int(0)),
                ("total_resolved_instances", PyValue::Int(0)),
                ("total_submitted_instances", PyValue::Int(0)),
                ("files", PyValue::List(Vec::new())),
                ("total_unresolved_ids", PyValue::List(Vec::new())),
                ("total_emptypatch_ids", PyValue::List(Vec::new())),
                ("total_resolved_ids", PyValue::List(Vec::new())),
                ("total_submitted_ids", PyValue::List(Vec::new())),
            ]),
        ),
    ])
}

pub fn copy_filtered(source: &Path, dest: &Path) -> Result<CopyReport, Error> {
    let patterns = read_dockerignore(&source.join(DOCKERIGNORE_FILE))?;
    let spec = Spec::new(&patterns)?;
    let files = spec.walk(source);
    fs::create_dir_all(dest)?;
    let mut copied = Vec::new();
    for rel in files {
        let from = source.join(&rel);
        if !fs::metadata(&from).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let to = dest.join(&rel);
        fs::create_dir_all(to.parent().unwrap_or(dest))?;
        fs::copy(&from, &to)?;
        copied.push(rel);
    }
    Ok(CopyReport {
        patterns: patterns.len(),
        copied,
    })
}

pub fn prepare_progenitor(
    source: &Path,
    dest: &Path,
    image: Option<(&Docker, &str)>,
    log: &Log,
) -> Result<CopyReport, Error> {
    let report = copy_filtered(source, dest)?;
    log.info(format!(
        "Copied {} files to {} ({} ignore patterns)",
        report.copied.len(),
        dest.display(),
        report.patterns
    ));
    let metadata: PathBuf = dest.join("..").join(NODE_METADATA_FILE);
    if !metadata.exists() {
        fs::write(&metadata, pyjson::dumps(&progenitor_metadata(), None))?;
    }
    if let Some((docker, tag)) = image {
        for name in [DOCKERIGNORE_FILE, DOCKERFILE] {
            fs::copy(source.join(name), dest.join(name))?;
        }
        log.info(format!("Building Docker image '{tag}'..."));
        docker.build(dest, tag, true, log)?;
    }
    Ok(report)
}
