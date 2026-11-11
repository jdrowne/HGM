mod agent;
mod common;
mod eval;
mod git;
mod llm;
mod parsers;
mod patch;
mod testrepo;
mod tools;
mod withtools;

use serde_json::Value;

#[test]
fn crate_is_linked() {
    assert_eq!(hgm_coding_agent::CRATE_NAME, "hgm-coding-agent");
}

fn fixture_docs() -> Vec<(String, Value)> {
    let mut docs = Vec::new();
    for entry in std::fs::read_dir(common::fixture_dir()).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let doc: Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            docs.push((path.display().to_string(), doc));
        }
    }
    docs
}

fn fixture_cases(doc: &Value) -> &Vec<Value> {
    doc["cases"].as_array().expect("cases array")
}

#[test]
fn golden_fixtures_exist() {
    assert!(!fixture_docs().is_empty());
}

#[test]
fn every_golden_fixture_has_cases() {
    let empty: Vec<String> = fixture_docs()
        .into_iter()
        .filter(|(_, doc)| fixture_cases(doc).is_empty())
        .map(|(path, _)| path)
        .collect();
    assert_eq!(empty, Vec::<String>::new());
}

#[test]
fn every_golden_fixture_case_is_named() {
    let unnamed: Vec<String> = fixture_docs()
        .into_iter()
        .filter(|(_, doc)| !fixture_cases(doc).iter().all(|c| c["name"].is_string()))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(unnamed, Vec::<String>::new());
}
