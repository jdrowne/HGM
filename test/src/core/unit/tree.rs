use hgm_core::tree::Tree;
use serde_json::{Value, json};

use crate::common::{assert_same, build_tree, cases, py};

#[test]
fn matches_the_python_tree() {
    for case in cases("tree") {
        let name = case["name"].as_str().unwrap();
        let tree = build_tree(&case["input"]);
        for (id, expected) in case["output"].as_object().unwrap() {
            let id: usize = id.parse().unwrap();
            let node = tree.get(id).unwrap();
            let what = format!("{name} node {id}");
            assert_same(&json!(node.num_evals()), &expected["num_evals"], &what);
            assert_same(
                &py(&hgm_core::pyjson::PyValue::Float(node.mean_utility())),
                &expected["mean_utility"],
                &what,
            );
            assert_same(&json!(tree.sub_tree(id)), &expected["sub_tree_ids"], &what);
            assert_same(&py(&node.save_as_dict()), &expected["save_as_dict"], &what);
            for (num_pseudo, evals) in expected["decendant_evals_by_num_pseudo"]
                .as_object()
                .unwrap()
            {
                let actual = tree.descendant_evals(id, num_pseudo.parse().unwrap());
                assert_same(
                    &json!(actual),
                    evals,
                    &format!("{what} num_pseudo {num_pseudo}"),
                );
            }
        }
    }
}

#[test]
fn descendant_evals_leave_every_node_unchanged() {
    let tree = build_tree(&crate::common::case("tree", "branching")["input"]);
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
fn nodes_keep_insertion_order_and_new_ids_count_up() {
    let mut tree = Tree::with_root();
    let a = tree.add_child(0, "a");
    let b = tree.add_child(0, "b");
    let a1 = tree.add_child(a, "a1");
    assert_eq!((a, b, a1), (1, 2, 3));
    let order: Vec<&str> = tree.nodes().map(|n| n.commit_id.as_str()).collect();
    assert_eq!(order, ["initial", "a", "b", "a1"]);
    assert_eq!(tree.sub_tree(0), [0, 1, 3, 2]);
    assert_eq!(tree.get(a).unwrap().children, [a1]);
}

#[test]
fn total_evals_sums_the_whole_tree() {
    let tree = build_tree(&crate::common::case("tree", "branching")["input"]);
    assert_eq!(tree.total_evals(), 15);
    assert_eq!(Tree::new().total_evals(), 0);
}

#[test]
fn a_node_with_no_evals_has_infinite_mean_and_is_not_expandable() {
    let tree = Tree::with_root();
    let root = tree.get(0).unwrap();
    assert!(root.mean_utility().is_infinite());
    assert!(!root.is_expandable());
    assert_eq!(
        py(&root.save_as_dict())["mean_utility"],
        Value::from("Infinity")
    );
}

#[test]
fn lookups_of_unknown_nodes_find_nothing() {
    let mut tree = Tree::new();
    assert!(tree.is_empty());
    assert_eq!(tree.len(), 0);
    assert!(tree.get_mut(7).is_none());
    assert!(tree.sub_tree(7).is_empty());
    assert!(tree.descendant_evals(7, 0).is_empty());
    let mut tree = Tree::with_root();
    let child = tree.add_child(0, "c");
    tree.get_mut(child).unwrap().utility_measures.push(1.0);
    assert_eq!(tree.len(), 2);
    assert!(!tree.is_empty());
    assert_eq!(tree.sub_tree(0), [0, child]);
}

#[test]
fn reinserting_an_id_and_linking_to_unknown_parents_change_nothing() {
    let mut tree = Tree::with_root();
    tree.insert("again", None, Some(0));
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.get(0).unwrap().commit_id, "again");
    let orphan = tree.add_child(9, "orphan");
    tree.link_children();
    assert_eq!(tree.sub_tree(0), [0]);
    assert_eq!(tree.get(orphan).unwrap().parent_id, Some(9));
}
