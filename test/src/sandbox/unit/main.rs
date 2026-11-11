mod docker;
mod error;
mod ignore;
mod source;

use std::path::PathBuf;

use serde_json::Value;

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/sandbox/unit/fixtures")
}

pub fn case(file: &str, name: &str) -> Value {
    let text = std::fs::read_to_string(fixtures_dir().join(format!("{file}.json"))).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no case {name} in {file}"))
        .clone()
}

pub fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_sandbox::CRATE_NAME, "rustyhgm-sandbox");
}
