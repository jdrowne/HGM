use std::fs;

use hgm_core::metadata::{
    OverallPerformance, get_all_performance, is_compiled_self_improve, load_hgm_metadata,
    load_init_evaluated_tasks, load_last_hgm_metadata, model_patch_paths, read_overall_performance,
    tree_from_record, update_metadata,
};
use serde_json::{Value, json};

use crate::common::{assert_same, build_tree, case, cases, fixtures_dir, py};

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn run_metadata_matches_the_python_byte_for_byte() {
    let fixture = case("run_metadata", "branching_tree_two_updates");
    let input = &fixture["input"];
    let output = &fixture["output"];
    let mut tree = build_tree(&case("tree", input["tree"].as_str().unwrap())["input"]);
    let dir = tempfile::tempdir().unwrap();
    for update in input["updates"].as_array().unwrap() {
        if let Some(id) = update["append_eval_to_node"].as_u64() {
            tree.get_mut(id as usize)
                .unwrap()
                .utility_measures
                .push(1.0);
        }
        update_metadata(
            dir.path(),
            &tree,
            update["n_task_evals"].as_u64().unwrap() as usize,
            &strings(&update["init_evaluated_tasks"]),
        )
        .unwrap();
    }
    for (written, key) in [
        ("hgm_metadata.jsonl", "jsonl_file"),
        ("init_evaluated_tasks.json", "init_evaluated_tasks_file"),
    ] {
        let actual = fs::read_to_string(dir.path().join(written)).unwrap();
        let expected =
            fs::read_to_string(fixtures_dir().join(output[key].as_str().unwrap())).unwrap();
        assert_eq!(actual, expected, "{written}");
    }
    let jsonl = fixtures_dir().join(output["jsonl_file"].as_str().unwrap());
    assert_same(
        &Value::Array(load_hgm_metadata(&jsonl).unwrap()),
        &output["load_all"],
        "load_all",
    );
    assert_same(
        &load_last_hgm_metadata(&jsonl).unwrap(),
        &output["load_last_only"],
        "load_last_only",
    );
    assert_eq!(
        load_init_evaluated_tasks(
            &fixtures_dir().join(output["init_evaluated_tasks_file"].as_str().unwrap())
        )
        .unwrap(),
        ["t1", "t2", "t3"]
    );
}

#[test]
fn a_saved_record_rebuilds_the_tree_shape() {
    let jsonl = fixtures_dir().join("run_metadata/hgm_metadata.jsonl");
    let tree = tree_from_record(&load_last_hgm_metadata(&jsonl).unwrap()).unwrap();
    let commits: Vec<&str> = tree.nodes().map(|n| n.commit_id.as_str()).collect();
    assert_eq!(commits, ["initial", "a", "b", "a1", "a2", "b1"]);
    assert_eq!(tree.sub_tree(0), [0, 1, 3, 4, 2, 5]);
    assert!(tree.nodes().all(|n| n.utility_measures.is_empty()));
}

#[test]
fn utility_measures_come_from_the_node_metadata() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("metadata.json"),
        r#"{"overall_performance": {"accuracy_score": 0.5, "total_resolved_instances": 2, "total_submitted_instances": 3, "total_submitted_ids": ["a", "b", "c"]}}"#,
    )
    .unwrap();
    let overall = read_overall_performance(dir.path()).unwrap();
    assert_eq!(overall.utility_measures(), [1.0, 1.0, 0.0]);
    assert_eq!(overall.total_submitted_ids, ["a", "b", "c"]);
    assert_eq!(
        OverallPerformance::default().utility_measures(),
        Vec::<f64>::new()
    );
}

#[test]
fn performance_matches_the_python_evo_utils() {
    let first = &cases("evo_utils")[0]["input"]["files"];
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in first.as_object().unwrap() {
        let text = match content {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        fs::write(dir.path().join(name), text).unwrap();
    }
    for case in cases("evo_utils") {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("get_all_performance_") {
            continue;
        }
        let (per_file, overall) =
            get_all_performance(dir.path(), case["input"]["run_keyword"].as_str().unwrap())
                .unwrap();
        assert_same(
            &py(&hgm_core::pyjson::PyValue::List(per_file)),
            &case["output"]["performance_results"],
            name,
        );
        assert_same(
            &py(&overall.to_py()),
            &case["output"]["overall_performance"],
            name,
        );
    }
}

#[test]
fn patch_paths_follow_the_parent_links() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    for case in cases("evo_utils") {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("get_model_patch_paths_") {
            continue;
        }
        let input = &case["input"];
        let no_patch = strings(&input["no_patch_file"]);
        for (commit, parent) in input["parent_links"].as_object().unwrap() {
            let dir = run.join(commit);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("metadata.json"),
                json!({"parent_commit": parent}).to_string(),
            )
            .unwrap();
            if !no_patch.contains(commit) {
                fs::write(dir.join("model_patch.diff"), format!("patch {commit}\n")).unwrap();
            }
        }
        let paths =
            model_patch_paths(tmp.path(), "run", input["parent_commit"].as_str().unwrap()).unwrap();
        let shown: Vec<String> = paths
            .iter()
            .map(|p| {
                let rel = p.strip_prefix(tmp.path()).unwrap();
                std::iter::once("<TMP>".to_owned())
                    .chain(
                        rel.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
                    )
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        assert_eq!(json!(shown), case["output"], "{name}");
    }
}

#[test]
fn compiled_self_improvement_matches_the_python() {
    for case in cases("evo_utils") {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("is_compiled_self_improve_") {
            continue;
        }
        let issues = case["input"]["num_swe_issues"][0].as_u64();
        assert_eq!(
            json!(is_compiled_self_improve(&case["input"]["metadata"], issues)),
            case["output"],
            "{name}"
        );
    }
    let ok = &case("evo_utils", "is_compiled_self_improve_ok")["input"]["metadata"];
    assert!(is_compiled_self_improve(ok, None));
}

#[test]
fn malformed_metadata_is_invalid_data() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("hgm_metadata.jsonl");
    fs::write(&empty, "").unwrap();
    let invalid = |e: std::io::Error| assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
    invalid(load_last_hgm_metadata(&empty).unwrap_err());
    fs::write(&empty, "{\"a\": }").unwrap();
    invalid(load_hgm_metadata(&empty).unwrap_err());
    let tasks = tmp.path().join("tasks.json");
    fs::write(&tasks, "{}").unwrap();
    invalid(load_init_evaluated_tasks(&tasks).unwrap_err());
    for record in [
        json!({}),
        json!({"nodes": [{"id": 1}]}),
        json!({"nodes": [{"commit_id": "c"}]}),
    ] {
        invalid(tree_from_record(&record).unwrap_err());
    }
    fs::write(tmp.path().join("metadata.json"), "{}").unwrap();
    invalid(read_overall_performance(tmp.path()).unwrap_err());
    fs::write(tmp.path().join("metadata.json"), "not json").unwrap();
    invalid(read_overall_performance(tmp.path()).unwrap_err());
}

#[test]
fn accuracy_is_resolved_over_submitted() {
    let mut perf = OverallPerformance::default();
    assert_eq!(perf.accuracy_score(), 0.0);
    perf.total_resolved_instances = 1;
    perf.total_submitted_instances = 4;
    assert_eq!(perf.accuracy_score(), 0.25);
}

#[test]
fn a_results_file_overrides_the_computed_fields_and_may_be_a_list() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("run_a.json"),
        r#"{"file": "other", "accuracy_score": 0.5, "resolved_instances": 1, "submitted_instances": 3}"#,
    )
    .unwrap();
    fs::write(tmp.path().join("run_b.json"), "[]").unwrap();
    fs::write(tmp.path().join("run_c.txt"), "{}").unwrap();
    let (per_file, overall) = get_all_performance(tmp.path(), "run").unwrap();
    assert_eq!(overall.files, ["run_a.json", "run_b.json"]);
    assert_eq!(overall.total_submitted_instances, 3);
    let first = py(&per_file[0]);
    assert_eq!(first["file"], "other");
    assert_eq!(first["accuracy_score"], 0.5);
    assert_eq!(py(&per_file[1])["accuracy_score"], 0);
    fs::write(tmp.path().join("run_d.json"), "{").unwrap();
    assert!(get_all_performance(tmp.path(), "run").is_err());
    assert!(get_all_performance(&tmp.path().join("missing"), "run").is_err());
}
