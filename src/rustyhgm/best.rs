use std::fs;
use std::path::Path;
use std::process::Command;

use rustyhgm_bench::polyglot_build::copy_tree;
use rustyhgm_core::metadata::{HGM_METADATA_FILE, load_last_rustyhgm_metadata, model_patch_paths};
use rustyhgm_core::tree::ROOT_COMMIT;
use serde_json::Value;
use statrs::distribution::{Beta, ContinuousCDF};

use crate::node::at;

pub const BEST_AGENT: &str = "best_agent";

#[derive(Clone, Debug, PartialEq)]
pub struct Belief {
    pub commit_id: String,
    pub successes: u64,
    pub evals: u64,
    pub quantile: f64,
}

pub fn quantile(successes: u64, evals: u64, epsilon: f64) -> f64 {
    let failures = evals - successes;
    Beta::new(1.0 + successes as f64, 1.0 + failures as f64)
        .map_or(f64::NAN, |beta| beta.inverse_cdf(epsilon))
}

fn belief(node: &Value, epsilon: f64) -> Result<Belief, String> {
    let commit_id = node
        .get("commit_id")
        .and_then(Value::as_str)
        .ok_or("node has no commit_id")?;
    let evals = node
        .get("num_evals")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("node {commit_id} has no num_evals"))?;
    let mean = node
        .get("mean_utility")
        .and_then(Value::as_f64)
        .filter(|m| (0.0..=1.0).contains(m))
        .unwrap_or(0.0);
    let successes = (mean * evals as f64).round() as u64;
    Ok(Belief {
        commit_id: commit_id.to_owned(),
        successes,
        evals,
        quantile: quantile(successes, evals, epsilon),
    })
}

pub fn check_epsilon(epsilon: f64) -> Result<(), String> {
    if epsilon > 0.0 && epsilon < 1.0 {
        Ok(())
    } else {
        Err(format!("epsilon must be between 0 and 1, got {epsilon}"))
    }
}

pub fn beliefs(record: &Value, epsilon: f64) -> Result<Vec<Belief>, String> {
    check_epsilon(epsilon)?;
    record
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or("record has no nodes list")?
        .iter()
        .map(|node| belief(node, epsilon))
        .collect()
}

pub fn best_belief(record: &Value, epsilon: f64) -> Result<Belief, String> {
    beliefs(record, epsilon)?
        .into_iter()
        .reduce(|best, b| if b.quantile > best.quantile { b } else { best })
        .ok_or_else(|| "the record lists no agents".to_owned())
}

pub fn pick(run_dir: &Path, epsilon: f64) -> Result<Belief, String> {
    let path = run_dir.join(HGM_METADATA_FILE);
    let record = load_last_rustyhgm_metadata(&path).map_err(|e| at(&path, e))?;
    best_belief(&record, epsilon)
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new("git")
        .args(["-c", "core.autocrlf=false", "-C"])
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn export(run_dir: &Path, commit_id: &str, dest: &Path) -> Result<(), String> {
    let seed = run_dir.join(ROOT_COMMIT).join("src");
    if !seed.is_dir() {
        return Err(format!("{}: the seed source is missing", seed.display()));
    }
    let patches = model_patch_paths(run_dir, "", commit_id).map_err(|e| at(run_dir, e))?;
    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| at(dest, e))?;
    }
    copy_tree(&seed, dest).map_err(|e| at(dest, e))?;
    if patches.is_empty() {
        return Ok(());
    }
    git(dest, &["init", "-q"])?;
    for patch in &patches {
        let patch = std::path::absolute(patch).map_err(|e| at(patch, e))?;
        git(dest, &["apply", &patch.to_string_lossy()]).map_err(|e| at(&patch, e))?;
    }
    let repo = dest.join(".git");
    fs::remove_dir_all(&repo).map_err(|e| at(&repo, e))
}
