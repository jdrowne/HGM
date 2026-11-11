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
    let tasks = swe_run_tasks();
    assert_eq!(
        tasks,
        strings(&output("swe_small_plus_medium_shuffled_seed_42"))
    );
    let mut doubled = tasks.clone();
    doubled.extend(tasks[..3].iter().cloned());
    assert_eq!(
        dedup(&doubled),
        strings(&output("swe_total_tasks_after_init_dedup"))
    );
}

#[test]
fn polyglot_run_tasks_match_the_python_order() {
    let tasks = polyglot_run_tasks();
    assert_eq!(tasks, strings(&output("polyglot_medium_plus_small")));
    assert_eq!(
        dedup(&tasks),
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
fn subsets_are_disjoint_where_the_python_assumes_it() {
    assert!(!swe_subset(Subset::Big).is_empty());
    assert!(polyglot_subset(Subset::Big).is_empty());
    let small = swe_subset(Subset::Small);
    assert!(
        swe_subset(Subset::Medium)
            .iter()
            .all(|t| !small.contains(t))
    );
}
