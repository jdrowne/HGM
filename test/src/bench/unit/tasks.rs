use hgm_bench::tasks::{
    Subset, dedup, get_eval_script, polyglot_run_tasks, polyglot_subset, swe_run_tasks, swe_subset,
};

use crate::common::{cases, strings};

fn output(name: &str) -> serde_json::Value {
    cases("task_order")
        .into_iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no case {name}"))["output"]
        .clone()
}

#[test]
fn swe_run_tasks_match_the_python_order() {
    assert_eq!(
        swe_run_tasks(),
        strings(&output("swe_small_plus_medium_shuffled_seed_42"))
    );
}

#[test]
fn deduplicating_swe_tasks_keeps_the_first_of_each() {
    let tasks = swe_run_tasks();
    let mut doubled = tasks.clone();
    doubled.extend(tasks[..3].iter().cloned());
    assert_eq!(
        dedup(&doubled),
        strings(&output("swe_total_tasks_after_init_dedup"))
    );
}

#[test]
fn polyglot_run_tasks_match_the_python_order() {
    assert_eq!(
        polyglot_run_tasks(),
        strings(&output("polyglot_medium_plus_small"))
    );
}

#[test]
fn deduplicating_polyglot_tasks_matches_the_python() {
    assert_eq!(
        dedup(&polyglot_run_tasks()),
        strings(&output("polyglot_total_tasks_after_init_dedup"))
    );
}

#[test]
fn eval_script_matches_the_python() {
    assert_eq!(
        get_eval_script(&["cd /repo", "cargo test"]),
        output("get_eval_script").as_str().unwrap()
    );
}

#[test]
fn the_big_swe_subset_has_tasks() {
    assert!(!swe_subset(Subset::Big).is_empty());
}

#[test]
fn the_big_polyglot_subset_is_empty() {
    assert!(polyglot_subset(Subset::Big).is_empty());
}

#[test]
fn the_small_and_medium_swe_subsets_are_disjoint() {
    let small = swe_subset(Subset::Small);
    assert!(
        swe_subset(Subset::Medium)
            .iter()
            .all(|t| !small.contains(t))
    );
}
