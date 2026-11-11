use std::path::PathBuf;

use serde_json::Value;

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_bench::CRATE_NAME, "rustyhgm-bench");
}

fn golden_cases() -> Vec<(String, Vec<Value>)> {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let doc: Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let cases = doc["cases"].as_array().expect("cases array").clone();
            found.push((path.display().to_string(), cases));
        }
    }
    found
}

#[test]
fn golden_fixtures_exist() {
    assert!(!golden_cases().is_empty());
}

#[test]
fn every_golden_fixture_has_cases() {
    let empty: Vec<String> = golden_cases()
        .into_iter()
        .filter(|(_, cases)| cases.is_empty())
        .map(|(path, _)| path)
        .collect();
    assert_eq!(empty, Vec::<String>::new());
}

#[test]
fn every_golden_case_has_a_name() {
    let unnamed: Vec<String> = golden_cases()
        .into_iter()
        .filter(|(_, cases)| !cases.iter().all(|c| c["name"].is_string()))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(unnamed, Vec::<String>::new());
}
