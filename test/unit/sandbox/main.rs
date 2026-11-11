//! Unit tests for `hgm-sandbox`.

#[test]
fn crate_is_linked() {
    assert_eq!(hgm_sandbox::CRATE_NAME, "hgm-sandbox");
}

/// Every golden fixture captured from the Python (test/golden/capture.py)
/// parses and has the expected shape.
#[test]
fn golden_fixtures_load() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/unit/sandbox/fixtures");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let doc: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let cases = doc["cases"].as_array().expect("cases array");
            assert!(!cases.is_empty(), "{}", path.display());
            assert!(
                cases.iter().all(|c| c["name"].is_string()),
                "{}",
                path.display()
            );
            n += 1;
        }
    }
    assert!(n > 0, "no fixtures in {}", dir.display());
}
