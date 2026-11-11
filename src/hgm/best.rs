use std::path::Path;

use hgm_core::metadata::{HGM_METADATA_FILE, load_last_hgm_metadata};
use serde_json::Value;
use statrs::distribution::{Beta, ContinuousCDF};

use crate::cli::BestArgs;
use crate::node::at;

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

pub fn beliefs(record: &Value, epsilon: f64) -> Result<Vec<Belief>, String> {
    if !(epsilon > 0.0 && epsilon < 1.0) {
        return Err(format!("epsilon must be between 0 and 1, got {epsilon}"));
    }
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

pub fn best(args: &BestArgs, root_dir: &Path) -> Result<(), String> {
    let path = root_dir.join(&args.run_dir).join(HGM_METADATA_FILE);
    let record = load_last_hgm_metadata(&path).map_err(|e| at(&path, e))?;
    let b = best_belief(&record, args.epsilon)?;
    println!(
        "{}\t{}\t{}/{}",
        b.commit_id, b.quantile, b.successes, b.evals
    );
    Ok(())
}
