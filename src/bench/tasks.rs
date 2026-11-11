use crate::pyrandom::PyRandom;

const SWE_SMALL: &str = include_str!("subsets/swe/small.json");
const SWE_MEDIUM: &str = include_str!("subsets/swe/medium.json");
const SWE_BIG: &str = include_str!("subsets/swe/big.json");
const POLYGLOT_SMALL: &str = include_str!("subsets/polyglot/small.json");
const POLYGLOT_MEDIUM: &str = include_str!("subsets/polyglot/medium.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subset {
    Small,
    Medium,
    Big,
}

fn parse(text: &str) -> Vec<String> {
    serde_json::from_str(text).expect("subset files are JSON lists of strings")
}

pub fn swe_subset(subset: Subset) -> Vec<String> {
    parse(match subset {
        Subset::Small => SWE_SMALL,
        Subset::Medium => SWE_MEDIUM,
        Subset::Big => SWE_BIG,
    })
}

pub fn polyglot_subset(subset: Subset) -> Vec<String> {
    match subset {
        Subset::Small => parse(POLYGLOT_SMALL),
        Subset::Medium => parse(POLYGLOT_MEDIUM),
        Subset::Big => Vec::new(),
    }
}

pub fn shuffled(mut tasks: Vec<String>) -> Vec<String> {
    PyRandom::seed(42).shuffle(&mut tasks);
    tasks
}

pub fn swe_run_tasks() -> Vec<String> {
    let mut tasks = swe_subset(Subset::Small);
    tasks.extend(swe_subset(Subset::Medium));
    shuffled(tasks)
}

pub fn polyglot_run_tasks() -> Vec<String> {
    let mut tasks = polyglot_subset(Subset::Medium);
    tasks.extend(polyglot_subset(Subset::Small));
    tasks
}

pub fn dedup(tasks: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tasks
        .iter()
        .filter(|t| seen.insert(t.as_str()))
        .cloned()
        .collect()
}

pub fn get_eval_script<S: AsRef<str>>(commands: &[S]) -> String {
    let mut lines = vec!["#!/bin/bash", "set -uxo pipefail"];
    lines.extend(commands.iter().map(AsRef::as_ref));
    lines.join("\n") + "\n"
}
