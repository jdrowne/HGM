use std::path::PathBuf;

use hgm_core::pyjson::{self, PyValue};
use hgm_core::tree::Tree;
use serde_json::Value;

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/core/unit/fixtures")
}

pub fn cases(name: &str) -> Vec<Value> {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["cases"].as_array().unwrap().clone()
}

pub fn case(name: &str, case: &str) -> Value {
    cases(name)
        .into_iter()
        .find(|c| c["name"] == case)
        .unwrap_or_else(|| panic!("{name}.json has no case {case}"))
}

pub fn py(value: &PyValue) -> Value {
    pyjson::parse_lenient(&pyjson::dumps(value, None)).unwrap()
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(_) | Value::String(_) => pyjson::f64_lenient(v),
        _ => None,
    }
}

pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((kp, p), (kq, q))| kp == kq && same(p, q))
        }
        (Value::Number(_), _) | (_, Value::Number(_)) => match (number(a), number(b)) {
            (Some(p), Some(q)) => p == q || (p.is_nan() && q.is_nan()),
            _ => false,
        },
        _ => a == b,
    }
}

pub fn differs(actual: &Value, expected: &Value, what: &str) -> Option<String> {
    (!same(actual, expected))
        .then(|| format!("{what}\n  actual:   {actual}\n  expected: {expected}"))
}

pub fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| pyjson::f64_lenient(x).unwrap())
        .collect()
}

pub fn build_tree(spec: &Value) -> Tree {
    let mut tree = Tree::new();
    for node in spec.as_array().unwrap() {
        let commit = node["commit_id"].as_str().unwrap();
        let id = match node["parent_id"].as_u64() {
            Some(parent) => tree.add_child(parent as usize, commit),
            None => tree.insert(commit, None, None),
        };
        tree.get_mut(id).unwrap().utility_measures = floats(&node["utility_measures"]);
    }
    tree
}
