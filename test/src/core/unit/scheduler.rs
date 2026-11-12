use std::collections::{HashMap, HashSet};

use rand::SeedableRng;
use rand::rngs::StdRng;
use rustyhgm_core::config::ExpansionStatistic;
use rustyhgm_core::scheduler::{
    NoExpandableNode, initial_expansions, pick_task, sample_budget, select_for_evaluation,
    select_for_expansion, should_expand,
};
use rustyhgm_core::tree::Tree;

use crate::common::{build_tree, case};

#[test]
fn expands_at_the_start_with_one_node_and_no_evals() {
    assert!(should_expand(0, 0.6, 1, 0));
}

#[test]
fn does_not_expand_when_a_node_is_already_being_built() {
    assert!(!should_expand(0, 0.6, 1, 1));
}

#[test]
fn does_not_expand_when_nodes_outnumber_the_evals() {
    assert!(!should_expand(0, 0.6, 2, 0));
}

#[test]
fn expands_when_evals_to_the_alpha_keep_up_with_the_nodes() {
    assert!(should_expand(4, 0.6, 3, 0));
}

#[test]
fn does_not_expand_with_evals_to_spare_while_building() {
    assert!(!should_expand(4, 0.6, 3, 1));
}

#[test]
fn initial_expansions_are_capped_at_five() {
    assert_eq!(initial_expansions(1, 16, 0.6), 5);
}

#[test]
fn initial_expansions_follow_the_budget_to_the_alpha() {
    assert_eq!(initial_expansions(1, 4, 0.6), 2);
}

#[test]
fn initial_expansions_subtract_existing_nodes() {
    assert_eq!(initial_expansions(3, 16, 0.6), 3);
}

#[test]
fn initial_expansions_stop_once_enough_nodes_exist() {
    assert_eq!(initial_expansions(9, 16, 0.6), 0);
}

#[test]
fn the_sample_budget_is_a_hundred_per_eval() {
    assert_eq!(sample_budget(800), 80000);
}

#[test]
fn expansion_of_an_unevaluated_tree_fails_fast() {
    let mut rng = StdRng::seed_from_u64(0);
    assert_eq!(
        select_for_expansion(
            &Tree::with_root(),
            10000,
            ExpansionStatistic::Clade,
            None,
            &mut rng
        ),
        Err(NoExpandableNode)
    );
}

#[test]
fn expansion_fails_fast_when_no_node_has_resolved_a_task() {
    let mut rng = StdRng::seed_from_u64(0);
    let mut failed = Tree::with_root();
    failed.get_mut(0).unwrap().utility_measures = vec![0.0, 0.0];
    assert_eq!(
        select_for_expansion(&failed, 10000, ExpansionStatistic::Clade, None, &mut rng),
        Err(NoExpandableNode)
    );
}

#[test]
fn no_expandable_node_points_at_a_fresh_run() {
    assert!(NoExpandableNode.to_string().contains("empty --output_dir"));
}

#[test]
fn expansion_only_picks_nodes_with_a_positive_finite_mean() {
    let tree = build_tree(&case("tree", "branching")["input"]);
    let mut rng = StdRng::seed_from_u64(3);
    let unexpandable: Vec<usize> = (0..200)
        .map(|_| {
            select_for_expansion(&tree, 10000, ExpansionStatistic::Clade, None, &mut rng).unwrap()
        })
        .filter(|&id| !tree.get(id).unwrap().is_expandable())
        .collect();
    assert_eq!(unexpandable, Vec::<usize>::new());
}

fn lucky_parent_of_a_failing_child() -> Tree {
    let mut tree = Tree::with_root();
    let parent = tree.add_child(0, "parent");
    let child = tree.add_child(parent, "child");
    let mut root = vec![0.0; 99];
    root.push(1.0);
    tree.get_mut(0).unwrap().utility_measures = root;
    tree.get_mut(parent).unwrap().utility_measures = vec![1.0; 50];
    tree.get_mut(child).unwrap().utility_measures = vec![0.0; 5000];
    tree
}

fn parent_picks(tree: &Tree, rng: &mut StdRng, statistic: ExpansionStatistic) -> usize {
    (0..200)
        .filter(|_| select_for_expansion(tree, 10000, statistic, None, rng).unwrap() == 1)
        .count()
}

#[test]
fn counting_only_its_own_evaluations_always_picks_the_lucky_parent() {
    let tree = lucky_parent_of_a_failing_child();
    let mut rng = StdRng::seed_from_u64(5);
    assert_eq!(parent_picks(&tree, &mut rng, ExpansionStatistic::Own), 200);
}

#[test]
fn counting_the_clade_lets_a_failing_child_drag_the_parent_down() {
    let tree = lucky_parent_of_a_failing_child();
    let mut rng = StdRng::seed_from_u64(5);
    parent_picks(&tree, &mut rng, ExpansionStatistic::Own);
    let clade = parent_picks(&tree, &mut rng, ExpansionStatistic::Clade);
    assert!((20..180).contains(&clade), "{clade}");
}

fn all_tasks() -> HashSet<String> {
    ["t1", "t2"].map(String::from).into()
}

#[test]
fn evaluation_skips_nodes_that_were_given_every_task() {
    let tree = build_tree(&case("tree", "branching")["input"]);
    let submitted: HashMap<usize, HashSet<String>> = [0, 1, 2, 3, 5]
        .into_iter()
        .map(|id| (id, all_tasks()))
        .collect();
    let mut rng = StdRng::seed_from_u64(1);
    let picks: Vec<Option<usize>> = (0..50)
        .map(|_| select_for_evaluation(&tree, &submitted, 2, None, &mut rng))
        .collect();
    assert_eq!(picks, vec![Some(4); 50]);
}

fn everything_done() -> HashMap<usize, HashSet<String>> {
    (0..6).map(|id| (id, all_tasks())).collect()
}

#[test]
fn evaluation_picks_nothing_when_every_node_has_every_task() {
    let tree = build_tree(&case("tree", "branching")["input"]);
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        select_for_evaluation(&tree, &everything_done(), 2, None, &mut rng),
        None
    );
}

#[test]
fn evaluation_of_an_empty_tree_picks_nothing() {
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        select_for_evaluation(&Tree::new(), &everything_done(), 2, None, &mut rng),
        None
    );
}

fn tasks() -> Vec<String> {
    ["a", "b", "c"].map(String::from).into()
}

fn submitted_a() -> HashSet<String> {
    ["a".to_string()].into()
}

#[test]
fn without_randomness_the_first_unsubmitted_task_is_picked() {
    let mut rng = StdRng::seed_from_u64(5);
    assert_eq!(
        pick_task(&tasks(), &submitted_a(), 0.0, &mut rng).as_deref(),
        Some("b")
    );
}

#[test]
fn a_random_task_is_picked_from_those_not_yet_submitted() {
    let mut rng = StdRng::seed_from_u64(5);
    let picked: HashSet<String> = (0..50)
        .map(|_| pick_task(&tasks(), &submitted_a(), 1.0, &mut rng).unwrap())
        .collect();
    assert!(
        picked.is_subset(&["b", "c"].map(String::from).into()),
        "{picked:?}"
    );
}

#[test]
fn no_task_is_picked_once_every_task_was_submitted() {
    let mut rng = StdRng::seed_from_u64(5);
    let everything: HashSet<String> = tasks().into_iter().collect();
    assert_eq!(pick_task(&tasks(), &everything, 1.0, &mut rng), None);
}
