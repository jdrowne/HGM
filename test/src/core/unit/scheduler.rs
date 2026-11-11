use std::collections::{HashMap, HashSet};

use hgm_core::scheduler::{
    NoExpandableNode, initial_expansions, pick_task, sample_budget, select_for_evaluation,
    select_for_expansion, should_expand,
};
use hgm_core::tree::Tree;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::common::{build_tree, case};

#[test]
fn expands_while_evals_to_the_alpha_keep_up_with_the_nodes() {
    assert!(should_expand(0, 0.6, 1, 0));
    assert!(!should_expand(0, 0.6, 1, 1));
    assert!(!should_expand(0, 0.6, 2, 0));
    assert!(should_expand(4, 0.6, 3, 0));
    assert!(!should_expand(4, 0.6, 3, 1));
}

#[test]
fn initial_expansions_are_capped_at_five() {
    assert_eq!(initial_expansions(1, 16, 0.6), 5);
    assert_eq!(initial_expansions(1, 4, 0.6), 2);
    assert_eq!(initial_expansions(3, 16, 0.6), 3);
    assert_eq!(initial_expansions(9, 16, 0.6), 0);
    assert_eq!(sample_budget(800), 80000);
}

#[test]
fn expansion_fails_fast_when_no_node_has_resolved_a_task() {
    let mut rng = StdRng::seed_from_u64(0);
    let unevaluated = Tree::with_root();
    assert_eq!(
        select_for_expansion(&unevaluated, 10000, None, &mut rng),
        Err(NoExpandableNode)
    );
    let mut failed = Tree::with_root();
    failed.get_mut(0).unwrap().utility_measures = vec![0.0, 0.0];
    assert_eq!(
        select_for_expansion(&failed, 10000, None, &mut rng),
        Err(NoExpandableNode)
    );
    assert!(NoExpandableNode.to_string().contains("initial_swe/<agent>"));
}

#[test]
fn expansion_only_picks_nodes_with_a_positive_finite_mean() {
    let tree = build_tree(&case("tree", "branching")["input"]);
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..200 {
        let id = select_for_expansion(&tree, 10000, None, &mut rng).unwrap();
        assert!(tree.get(id).unwrap().is_expandable(), "picked node {id}");
    }
}

#[test]
fn evaluation_skips_nodes_that_were_given_every_task() {
    let tree = build_tree(&case("tree", "branching")["input"]);
    let all: HashSet<String> = ["t1", "t2"].map(String::from).into();
    let submitted: HashMap<usize, HashSet<String>> = [0, 1, 2, 3, 5]
        .into_iter()
        .map(|id| (id, all.clone()))
        .collect();
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..50 {
        assert_eq!(
            select_for_evaluation(&tree, &submitted, 2, None, &mut rng),
            Some(4)
        );
    }
    let done: HashMap<usize, HashSet<String>> = (0..6).map(|id| (id, all.clone())).collect();
    assert_eq!(select_for_evaluation(&tree, &done, 2, None, &mut rng), None);
    assert_eq!(
        select_for_evaluation(&Tree::new(), &done, 2, None, &mut rng),
        None
    );
}

#[test]
fn a_task_is_picked_from_those_not_yet_submitted() {
    let tasks: Vec<String> = ["a", "b", "c"].map(String::from).into();
    let submitted: HashSet<String> = ["a".to_string()].into();
    let mut rng = StdRng::seed_from_u64(5);
    assert_eq!(
        pick_task(&tasks, &submitted, 0.0, &mut rng).as_deref(),
        Some("b")
    );
    for _ in 0..50 {
        let t = pick_task(&tasks, &submitted, 1.0, &mut rng).unwrap();
        assert!(t == "b" || t == "c");
    }
    let everything: HashSet<String> = tasks.iter().cloned().collect();
    assert_eq!(pick_task(&tasks, &everything, 1.0, &mut rng), None);
}
