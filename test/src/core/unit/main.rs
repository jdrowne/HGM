mod common;
mod config;
mod log;
mod metadata;
mod pool;
mod prompts;
mod pyjson;
mod sampling;
mod scheduler;
mod tree;

use std::path::PathBuf;

use serde_json::Value;

fn golden_fixtures() -> Vec<(PathBuf, Value)> {
    let dir = common::fixtures_dir();
    std::fs::read_dir(&dir)
        .expect("fixtures dir")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let doc: Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (path, doc)
        })
        .collect()
}

fn golden_cases(doc: &Value) -> &Vec<Value> {
    doc["cases"].as_array().expect("cases array")
}

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_core::CRATE_NAME, "rustyhgm-core");
}

#[test]
fn golden_fixtures_exist() {
    assert!(!golden_fixtures().is_empty(), "no fixtures");
}

#[test]
fn every_golden_fixture_has_cases() {
    let empty: Vec<PathBuf> = golden_fixtures()
        .into_iter()
        .filter(|(_, doc)| golden_cases(doc).is_empty())
        .map(|(path, _)| path)
        .collect();
    assert_eq!(empty, Vec::<PathBuf>::new());
}

#[test]
fn every_golden_fixture_case_has_a_name() {
    let unnamed: Vec<PathBuf> = golden_fixtures()
        .into_iter()
        .filter(|(_, doc)| !golden_cases(doc).iter().all(|c| c["name"].is_string()))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(unnamed, Vec::<PathBuf>::new());
}
