use std::fs;
use std::path::PathBuf;

use hgm_bench::report::{default_run_ids, load_predictions, preds_to_jsonl};
use hgm_bench::swe::{next_run_name, prediction};
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
    for name in ["prediction_json", "empty_prediction_json"] {
        let c = case(name);
        assert_eq!(
            prediction_of(&c["input"]),
            c["output"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn predictions_jsonl_matches_the_python() {
    let c = case("preds_to_jsonl_sorted_lines");
    let tmp = tempfile::tempdir().unwrap();
    for (name, body) in c["input"].as_object().unwrap() {
        fs::write(tmp.path().join(name), body.to_string()).unwrap();
    }
    let preds = load_predictions(&[tmp.path().to_path_buf()]).unwrap();
    assert_eq!(preds.keys().collect::<Vec<_>>(), ["inst-a", "inst-c"]);
    assert!(
        preds["inst-a"]["json_fname"]
            .as_str()
            .unwrap()
            .ends_with("a.json")
    );
    let path = preds_to_jsonl(tmp.path(), &preds).unwrap();
    assert_eq!(path, tmp.path().join("all_preds.jsonl"));
    let text = fs::read_to_string(path).unwrap();
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
    let in_order = lines.clone();
    lines.sort();
    assert_eq!(lines, strings(&c["output"]));
    assert!(in_order[0].contains("inst-a"));
}

#[test]
fn load_predictions_rejects_a_missing_path() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(load_predictions(&[tmp.path().join("nope")]).is_err());
    let file = tmp.path().join("one.json");
    fs::write(
        &file,
        r#"{"instance_id": "x", "model_name_or_path": "m", "model_patch": ""}"#,
    )
    .unwrap();
    assert_eq!(load_predictions(&[file]).unwrap().len(), 1);
}

#[test]
fn preds_to_jsonl_rejects_mixed_models_and_empty_sets() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(preds_to_jsonl(tmp.path(), &serde_json::Map::new()).is_err());
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
fn run_names_count_earlier_runs_of_the_model() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(next_run_name(tmp.path(), "agent").unwrap(), "agent_1");
    fs::create_dir(tmp.path().join("agent_1")).unwrap();
    fs::create_dir(tmp.path().join("other_1")).unwrap();
    fs::write(tmp.path().join("agent_file"), "").unwrap();
    assert_eq!(next_run_name(tmp.path(), "agent").unwrap(), "agent_2");
    assert!(next_run_name(&PathBuf::from("/definitely/not/here"), "agent").is_err());
}

#[test]
fn run_ids_are_zero_padded() {
    assert_eq!(default_run_ids(3), ["000", "001", "002"]);
}
