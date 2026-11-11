use hgm_core::config::ExpansionStatistic;
use hgm_core::pyjson::PyValue;
use hgm_core::tree::Tree;
use serde_json::{Value, json};

use crate::common::{build_tree, case, cases, differs, py};

fn node_mismatches(check: impl Fn(&Tree, usize, &Value, &str) -> Vec<String>) -> Vec<String> {
    cases("tree")
        .iter()
        .flat_map(|case| {
            let name = case["name"].as_str().unwrap();
            let tree = build_tree(&case["input"]);
            case["output"]
                .as_object()
                .unwrap()
                .iter()
                .flat_map(|(id, expected)| {
                    let id: usize = id.parse().unwrap();
                    check(&tree, id, expected, &format!("{name} node {id}"))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn eval_counts_match_the_python_tree() {
    let mismatches = node_mismatches(|tree, id, expected, what| {
        let node = tree.get(id).unwrap();
        differs(&json!(node.num_evals()), &expected["num_evals"], what)
            .into_iter()
            .collect()
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn mean_utilities_match_the_python_tree() {
    let mismatches = node_mismatches(|tree, id, expected, what| {
        let node = tree.get(id).unwrap();
        differs(
            &py(&PyValue::Float(node.mean_utility())),
            &expected["mean_utility"],
            what,
        )
        .into_iter()
        .collect()
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn sub_trees_match_the_python_tree() {
    let mismatches = node_mismatches(|tree, id, expected, what| {
        differs(&json!(tree.sub_tree(id)), &expected["sub_tree_ids"], what)
            .into_iter()
            .collect()
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn saved_nodes_match_the_python_tree() {
    let mismatches = node_mismatches(|tree, id, expected, what| {
        let node = tree.get(id).unwrap();
        differs(&py(&node.save_as_dict()), &expected["save_as_dict"], what)
            .into_iter()
            .collect()
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn descendant_evals_match_the_python_tree() {
    let mismatches = node_mismatches(|tree, id, expected, what| {
        expected["decendant_evals_by_num_pseudo"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(num_pseudo, evals)| {
                let actual = tree.descendant_evals(id, num_pseudo.parse().unwrap());
                differs(
                    &json!(actual),
                    evals,
                    &format!("{what} num_pseudo {num_pseudo}"),
                )
            })
            .collect()
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

fn branching() -> Tree {
    build_tree(&case("tree", "branching")["input"])
}

#[test]
fn descendant_evals_leave_every_node_unchanged() {
    let tree = branching();
    let before = tree.clone();
    for _ in 0..3 {
        for id in tree.sub_tree(0) {
            tree.descendant_evals(id, 10000);
            tree.descendant_evals(id, 1);
        }
    }
    assert_eq!(tree, before);
}

#[test]
fn clade_expansion_evals_are_the_descendant_evals() {
    let tree = branching();
    let ids = tree.sub_tree(0);
    let clade: Vec<Vec<f64>> = ids
        .iter()
        .map(|&id| tree.expansion_evals(id, 3, ExpansionStatistic::Clade))
        .collect();
    let descendants: Vec<Vec<f64>> = ids.iter().map(|&id| tree.descendant_evals(id, 3)).collect();
    assert_eq!(clade, descendants);
}

#[test]
fn own_expansion_evals_are_the_node_measures() {
    let tree = branching();
    let ids = tree.sub_tree(0);
    let own: Vec<Vec<f64>> = ids
        .iter()
        .map(|&id| tree.expansion_evals(id, 3, ExpansionStatistic::Own))
        .collect();
    let measures: Vec<Vec<f64>> = ids
        .iter()
        .map(|&id| tree.get(id).unwrap().utility_measures.clone())
        .collect();
    assert_eq!(own, measures);
}

#[test]
fn own_expansion_evals_of_an_unknown_node_are_empty() {
    assert!(
        branching()
            .expansion_evals(99, 3, ExpansionStatistic::Own)
            .is_empty()
    );
}

fn small_tree() -> (Tree, [usize; 3]) {
    let mut tree = Tree::with_root();
    let a = tree.add_child(0, "a");
    let b = tree.add_child(0, "b");
    let a1 = tree.add_child(a, "a1");
    (tree, [a, b, a1])
}

#[test]
fn new_ids_count_up() {
    assert_eq!(small_tree().1, [1, 2, 3]);
}

#[test]
fn nodes_keep_insertion_order() {
    let (tree, _) = small_tree();
    let order: Vec<&str> = tree.nodes().map(|n| n.commit_id.as_str()).collect();
    assert_eq!(order, ["initial", "a", "b", "a1"]);
}

#[test]
fn a_sub_tree_lists_each_clade_depth_first() {
    assert_eq!(small_tree().0.sub_tree(0), [0, 1, 3, 2]);
}

#[test]
fn a_node_lists_its_children() {
    let (tree, [a, _, a1]) = small_tree();
    assert_eq!(tree.get(a).unwrap().children, [a1]);
}

#[test]
fn total_evals_sums_the_whole_tree() {
    assert_eq!(branching().total_evals(), 15);
}

#[test]
fn an_empty_tree_has_no_evals() {
    assert_eq!(Tree::new().total_evals(), 0);
}

#[test]
fn a_node_with_no_evals_has_infinite_mean() {
    assert!(
        Tree::with_root()
            .get(0)
            .unwrap()
            .mean_utility()
            .is_infinite()
    );
}

#[test]
fn a_node_with_no_evals_is_not_expandable() {
    assert!(!Tree::with_root().get(0).unwrap().is_expandable());
}

#[test]
fn a_node_with_no_evals_saves_an_infinite_mean() {
    assert_eq!(
        py(&Tree::with_root().get(0).unwrap().save_as_dict())["mean_utility"],
        Value::from("Infinity")
    );
}

#[test]
fn a_new_tree_is_empty() {
    assert!(Tree::new().is_empty());
}

#[test]
fn a_new_tree_has_no_nodes() {
    assert_eq!(Tree::new().len(), 0);
}

#[test]
fn an_unknown_node_cannot_be_changed() {
    assert!(Tree::new().get_mut(7).is_none());
}

#[test]
fn an_unknown_node_has_an_empty_sub_tree() {
    assert!(Tree::new().sub_tree(7).is_empty());
}

#[test]
fn an_unknown_node_has_no_descendant_evals() {
    assert!(Tree::new().descendant_evals(7, 0).is_empty());
}

fn measured_child() -> (Tree, usize) {
    let mut tree = Tree::with_root();
    let child = tree.add_child(0, "c");
    tree.get_mut(child).unwrap().utility_measures.push(1.0);
    (tree, child)
}

#[test]
fn a_tree_counts_its_nodes() {
    assert_eq!(measured_child().0.len(), 2);
}

#[test]
fn a_tree_with_nodes_is_not_empty() {
    assert!(!measured_child().0.is_empty());
}

#[test]
fn a_sub_tree_includes_a_changed_child() {
    let (tree, child) = measured_child();
    assert_eq!(tree.sub_tree(0), [0, child]);
}

fn reinserted() -> Tree {
    let mut tree = Tree::with_root();
    tree.insert("again", None, Some(0));
    tree
}

#[test]
fn reinserting_an_id_keeps_one_node() {
    assert_eq!(reinserted().len(), 1);
}

#[test]
fn reinserting_an_id_replaces_its_commit() {
    assert_eq!(reinserted().get(0).unwrap().commit_id, "again");
}

fn with_orphan() -> (Tree, usize) {
    let mut tree = Tree::with_root();
    let orphan = tree.add_child(9, "orphan");
    tree.link_children();
    (tree, orphan)
}

#[test]
fn linking_to_an_unknown_parent_leaves_the_root_alone() {
    assert_eq!(with_orphan().0.sub_tree(0), [0]);
}

#[test]
fn linking_to_an_unknown_parent_keeps_the_parent_id() {
    let (tree, orphan) = with_orphan();
    assert_eq!(tree.get(orphan).unwrap().parent_id, Some(9));
}
