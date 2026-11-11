use std::fs;
use std::path::PathBuf;

use rustyhgm_bench::report::{default_run_ids, load_predictions, preds_to_jsonl};
use rustyhgm_bench::swe::{next_run_name, prediction};
use serde_json::Value;

use crate::common::{cases, strings};

fn case(name: &str) -> Value {
    cases("swe_report")
        .into_iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no case {name}"))
}

fn prediction_of(input: &Value) -> String {
    prediction(
        input["model_name_or_path"].as_str().unwrap(),
        input["instance_id"].as_str().unwrap(),
        input["model_patch"].as_str().unwrap(),
        &strings(&input["proposed_model_patches"]),
    )
}

#[test]
fn prediction_files_match_the_python() {
    let c = case("prediction_json");
    assert_eq!(prediction_of(&c["input"]), c["output"].as_str().unwrap());
}

#[test]
fn empty_prediction_files_match_the_python() {
    let c = case("empty_prediction_json");
    assert_eq!(prediction_of(&c["input"]), c["output"].as_str().unwrap());
}

fn written() -> (tempfile::TempDir, serde_json::Map<String, Value>) {
    let c = case("preds_to_jsonl_sorted_lines");
    let tmp = tempfile::tempdir().unwrap();
    for (name, body) in c["input"].as_object().unwrap() {
        fs::write(tmp.path().join(name), body.to_string()).unwrap();
    }
    let preds = load_predictions(&[tmp.path().to_path_buf()]).unwrap();
    (tmp, preds)
}

fn jsonl_lines() -> Vec<String> {
    let (tmp, preds) = written();
    let path = preds_to_jsonl(tmp.path(), &preds).unwrap();
    let text = fs::read_to_string(path).unwrap();
    text.split_inclusive('\n').map(str::to_owned).collect()
}

#[test]
fn predictions_load_by_instance() {
    let (_tmp, preds) = written();
    assert_eq!(preds.keys().collect::<Vec<_>>(), ["inst-a", "inst-c"]);
}

#[test]
fn loaded_predictions_name_their_file() {
    let (_tmp, preds) = written();
    assert!(
        preds["inst-a"]["json_fname"]
            .as_str()
            .unwrap()
            .ends_with("a.json")
    );
}

#[test]
fn predictions_jsonl_lands_beside_the_predictions() {
    let (tmp, preds) = written();
    assert_eq!(
        preds_to_jsonl(tmp.path(), &preds).unwrap(),
        tmp.path().join("all_preds.jsonl")
    );
}

#[test]
fn predictions_jsonl_matches_the_python() {
    let c = case("preds_to_jsonl_sorted_lines");
    let mut lines = jsonl_lines();
    lines.sort();
    assert_eq!(lines, strings(&c["output"]));
}

#[test]
fn predictions_jsonl_lists_instances_in_order() {
    assert!(jsonl_lines()[0].contains("inst-a"));
}

#[test]
fn load_predictions_rejects_a_missing_path() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(load_predictions(&[tmp.path().join("nope")]).is_err());
}

#[test]
fn load_predictions_reads_a_single_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("one.json");
    fs::write(
        &file,
        r#"{"instance_id": "x", "model_name_or_path": "m", "model_patch": ""}"#,
    )
    .unwrap();
    assert_eq!(load_predictions(&[file]).unwrap().len(), 1);
}

#[test]
fn preds_to_jsonl_rejects_an_empty_set() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(preds_to_jsonl(tmp.path(), &serde_json::Map::new()).is_err());
}

#[test]
fn preds_to_jsonl_rejects_mixed_models() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, model) in [("a.json", "m1"), ("b.json", "m2")] {
        let body = format!(
            r#"{{"instance_id": "{name}", "model_name_or_path": "{model}", "model_patch": ""}}"#
        );
        fs::write(tmp.path().join(name), body).unwrap();
    }
    let preds = load_predictions(&[tmp.path().to_path_buf()]).unwrap();
    assert!(preds_to_jsonl(tmp.path(), &preds).is_err());
}

#[test]
fn the_first_run_of_a_model_is_numbered_one() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(next_run_name(tmp.path(), "harness"), "harness_1");
}

#[test]
fn run_names_count_earlier_runs_of_the_model() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("harness_1")).unwrap();
    fs::create_dir(tmp.path().join("other_1")).unwrap();
    fs::write(tmp.path().join("harness_file"), "").unwrap();
    assert_eq!(next_run_name(tmp.path(), "harness"), "harness_2");
}

#[test]
fn a_missing_run_folder_starts_at_one() {
    assert_eq!(
        next_run_name(&PathBuf::from("/definitely/not/here"), "harness"),
        "harness_1"
    );
}

#[test]
fn run_ids_are_zero_padded() {
    assert_eq!(default_run_ids(3), ["000", "001", "002"]);
}

fn load_error(contents: &[u8]) -> (String, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bad.json");
    fs::write(&path, contents).unwrap();
    (
        load_predictions(std::slice::from_ref(&path)).unwrap_err(),
        path,
    )
}

#[test]
fn an_unreadable_prediction_file_is_named_in_the_error() {
    let (e, path) = load_error(&[0xff, 0xfe]);
    assert!(e.starts_with(&path.display().to_string()), "{e}");
}

#[test]
fn a_junk_prediction_file_is_named_in_the_error() {
    let (e, path) = load_error(b"{");
    assert!(e.starts_with(&path.display().to_string()), "{e}");
}

#[test]
fn an_unwritable_jsonl_path_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("a.json"),
        r#"{"instance_id": "a", "model_name_or_path": "m", "model_patch": ""}"#,
    )
    .unwrap();
    fs::create_dir(tmp.path().join("all_preds.jsonl")).unwrap();
    let preds = load_predictions(&[tmp.path().to_path_buf()]).unwrap();
    let e = preds_to_jsonl(tmp.path(), &preds).unwrap_err();
    assert!(e.contains("all_preds.jsonl"), "{e}");
}
