use std::collections::{HashMap, HashSet};
use std::fmt;

use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

use crate::config::ExpansionStatistic;
use crate::sampling::{CoolDown, thompson_sample};
use crate::tree::Tree;

pub const MAX_INITIAL_EXPANSIONS: usize = 5;

pub const SAMPLES_PER_TASK_EVAL: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoExpandableNode;

impl fmt::Display for NoExpandableNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "no node can be expanded: none has a finite mean utility above 0. \
             The initial agent resolved no task, or was never evaluated; \
             check the API keys and Docker, or delete initial_swe/<agent> to re-evaluate it",
        )
    }
}

impl std::error::Error for NoExpandableNode {}

pub fn should_expand(
    n_task_evals: usize,
    alpha: f64,
    n_nodes: usize,
    n_pending_expands: usize,
) -> bool {
    (n_task_evals as f64).powf(alpha) >= (n_nodes.saturating_sub(1) + n_pending_expands) as f64
}

pub fn initial_expansions(n_nodes: usize, max_workers: usize, alpha: f64) -> usize {
    let cap = MAX_INITIAL_EXPANSIONS.min((max_workers as f64).powf(alpha) as usize);
    cap.saturating_sub(n_nodes.saturating_sub(1))
}

pub fn sample_budget(max_task_evals: usize) -> usize {
    max_task_evals * SAMPLES_PER_TASK_EVAL
}

pub fn select_for_expansion<R: Rng + ?Sized>(
    tree: &Tree,
    num_pseudo: usize,
    statistic: ExpansionStatistic,
    cool_down: Option<CoolDown>,
    rng: &mut R,
) -> Result<usize, NoExpandableNode> {
    let candidates: Vec<usize> = tree
        .nodes()
        .filter(|n| n.is_expandable())
        .map(|n| n.id)
        .collect();
    let evals: Vec<Vec<f64>> = candidates
        .iter()
        .map(|&id| tree.expansion_evals(id, num_pseudo, statistic))
        .collect();
    thompson_sample(&evals, cool_down, rng)
        .map(|i| candidates[i])
        .ok_or(NoExpandableNode)
}

pub fn select_for_evaluation<R: Rng + ?Sized>(
    tree: &Tree,
    submitted_ids: &HashMap<usize, HashSet<String>>,
    total_num_tasks: usize,
    cool_down: Option<CoolDown>,
    rng: &mut R,
) -> Option<usize> {
    let root = tree.nodes().next()?.id;
    let candidates: Vec<usize> = tree
        .sub_tree(root)
        .into_iter()
        .filter(|id| submitted_ids.get(id).map_or(0, HashSet::len) < total_num_tasks)
        .collect();
    let evals: Vec<Vec<f64>> = candidates
        .iter()
        .map(|&id| {
            tree.get(id)
                .map(|n| n.utility_measures.clone())
                .unwrap_or_default()
        })
        .collect();
    thompson_sample(&evals, cool_down, rng).map(|i| candidates[i])
}

pub fn pick_task<R: Rng + ?Sized>(
    total_tasks: &[String],
    submitted: &HashSet<String>,
    eval_random_level: f64,
    rng: &mut R,
) -> Option<String> {
    let available: Vec<&String> = total_tasks
        .iter()
        .filter(|t| !submitted.contains(*t))
        .collect();
    if available.is_empty() {
        return None;
    }
    let task = if rng.random::<f64>() < eval_random_level {
        available.choose(rng).copied()
    } else {
        available.first().copied()
    };
    task.cloned()
}
