use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rustyhgm_core::metadata::{
    OverallPerformance, get_all_performance, is_compiled_self_improve, load_init_evaluated_tasks,
    load_last_rustyhgm_metadata, load_rustyhgm_metadata, model_patch_paths,
    read_overall_performance, run_id_from_record, tree_from_record, update_metadata,
};
use rustyhgm_core::pyjson::PyValue;
use rustyhgm_core::tree::Tree;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::common::{build_tree, case, cases, differs, fixtures_dir, py};

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn run_output() -> Value {
    case("run_metadata", "branching_tree_two_updates")["output"].clone()
}

fn fixture_file(output: &Value, key: &str) -> PathBuf {
    fixtures_dir().join(output[key].as_str().unwrap())
}

fn written_run() -> TempDir {
    let input = case("run_metadata", "branching_tree_two_updates")["input"].clone();
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
            input["run_id"].as_str().unwrap(),
            &tree,
            update["n_task_evals"].as_u64().unwrap() as usize,
            &strings(&update["init_evaluated_tasks"]),
        )
        .unwrap();
    }
    dir
}

fn written_matches_fixture(written: &str, key: &str) -> (String, String) {
    let dir = written_run();
    (
        fs::read_to_string(dir.path().join(written)).unwrap(),
        fs::read_to_string(fixture_file(&run_output(), key)).unwrap(),
    )
}

#[test]
fn run_metadata_jsonl_matches_the_python_byte_for_byte() {
    let (actual, expected) = written_matches_fixture("rustyhgm_metadata.jsonl", "jsonl_file");
    assert_eq!(actual, expected);
}

#[test]
fn run_metadata_init_tasks_match_the_python_byte_for_byte() {
    let (actual, expected) =
        written_matches_fixture("init_evaluated_tasks.json", "init_evaluated_tasks_file");
    assert_eq!(actual, expected);
}

#[test]
fn loading_every_record_matches_the_python() {
    let output = run_output();
    let all = Value::Array(load_rustyhgm_metadata(&fixture_file(&output, "jsonl_file")).unwrap());
    assert_eq!(differs(&all, &output["load_all"], "load_all"), None);
}

#[test]
fn loading_the_last_record_matches_the_python() {
    let output = run_output();
    let last = load_last_rustyhgm_metadata(&fixture_file(&output, "jsonl_file")).unwrap();
    assert_eq!(
        differs(&last, &output["load_last_only"], "load_last_only"),
        None
    );
}

#[test]
fn loading_init_evaluated_tasks_reads_the_list() {
    let output = run_output();
    assert_eq!(
        load_init_evaluated_tasks(&fixture_file(&output, "init_evaluated_tasks_file")).unwrap(),
        ["t1", "t2", "t3"]
    );
}

fn saved_tree() -> Tree {
    let jsonl = fixtures_dir().join("run_metadata/rustyhgm_metadata.jsonl");
    tree_from_record(&load_last_rustyhgm_metadata(&jsonl).unwrap()).unwrap()
}

#[test]
fn a_saved_record_rebuilds_the_commits_in_order() {
    let tree = saved_tree();
    let commits: Vec<&str> = tree.nodes().map(|n| n.commit_id.as_str()).collect();
    assert_eq!(commits, ["progenitor", "a", "b", "a1", "a2", "b1"]);
}

#[test]
fn a_saved_record_rebuilds_the_tree_shape() {
    assert_eq!(saved_tree().sub_tree(0), [0, 1, 3, 4, 2, 5]);
}

#[test]
fn a_saved_record_rebuilds_nodes_without_measures() {
    assert!(saved_tree().nodes().all(|n| n.utility_measures.is_empty()));
}

fn node_performance() -> OverallPerformance {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("metadata.json"),
        r#"{"overall_performance": {"accuracy_score": 0.5, "total_resolved_instances": 2, "total_submitted_instances": 3, "total_submitted_ids": ["a", "b", "c"]}}"#,
    )
    .unwrap();
    read_overall_performance(dir.path()).unwrap()
}

#[test]
fn utility_measures_come_from_the_node_metadata() {
    assert_eq!(node_performance().utility_measures(), [1.0, 1.0, 0.0]);
}

#[test]
fn submitted_ids_come_from_the_node_metadata() {
    assert_eq!(node_performance().total_submitted_ids, ["a", "b", "c"]);
}

#[test]
fn default_performance_has_no_utility_measures() {
    assert_eq!(
        OverallPerformance::default().utility_measures(),
        Vec::<f64>::new()
    );
}

fn evo_cases(prefix: &str) -> Vec<Value> {
    cases("evo_utils")
        .into_iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with(prefix))
        .collect()
}

fn performance_dir() -> TempDir {
    let first = &cases("evo_utils")[0]["input"]["files"];
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in first.as_object().unwrap() {
        let text = match content {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        fs::write(dir.path().join(name), text).unwrap();
    }
    dir
}

fn performance_mismatches(
    pick: fn(Vec<PyValue>, OverallPerformance) -> (Value, &'static str),
) -> Vec<String> {
    let dir = performance_dir();
    evo_cases("get_all_performance_")
        .iter()
        .filter_map(|case| {
            let (per_file, overall) =
                get_all_performance(dir.path(), case["input"]["run_keyword"].as_str().unwrap())
                    .unwrap();
            let (actual, key) = pick(per_file, overall);
            differs(
                &actual,
                &case["output"][key],
                case["name"].as_str().unwrap(),
            )
        })
        .collect()
}

#[test]
fn per_file_performance_matches_the_python_evo_utils() {
    let mismatches =
        performance_mismatches(|per_file, _| (py(&PyValue::List(per_file)), "performance_results"));
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn overall_performance_matches_the_python_evo_utils() {
    let mismatches =
        performance_mismatches(|_, overall| (py(&overall.to_py()), "overall_performance"));
    assert_eq!(mismatches, Vec::<String>::new());
}

fn shown_paths(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap();
            std::iter::once("<TMP>".to_owned())
                .chain(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned()),
                )
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect()
}

#[test]
fn patch_paths_follow_the_parent_links() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    let mismatches: Vec<String> = evo_cases("get_model_patch_paths_")
        .iter()
        .filter_map(|case| {
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
                model_patch_paths(tmp.path(), "run", input["parent_commit"].as_str().unwrap())
                    .unwrap();
            let shown = json!(shown_paths(tmp.path(), &paths));
            (shown != case["output"]).then(|| format!("{}: {shown}", case["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn compiled_self_improvement_matches_the_python() {
    let mismatches: Vec<String> = evo_cases("is_compiled_self_improve_")
        .iter()
        .filter_map(|case| {
            let issues = case["input"]["num_swe_issues"][0].as_u64();
            let actual = json!(is_compiled_self_improve(&case["input"]["metadata"], issues));
            (actual != case["output"]).then(|| format!("{}: {actual}", case["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn compiled_self_improvement_without_an_issue_count_is_compiled() {
    let ok = &case("evo_utils", "is_compiled_self_improve_ok")["input"]["metadata"];
    assert!(is_compiled_self_improve(ok, None));
}

fn file_with(name: &str, text: &str) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    fs::write(&path, text).unwrap();
    (tmp, path)
}

#[test]
fn an_empty_metadata_file_has_no_last_record() {
    let (_tmp, path) = file_with("rustyhgm_metadata.jsonl", "");
    assert_eq!(
        load_last_rustyhgm_metadata(&path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_broken_metadata_line_is_invalid_data() {
    let (_tmp, path) = file_with("rustyhgm_metadata.jsonl", "{\"a\": }");
    assert_eq!(
        load_rustyhgm_metadata(&path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn init_evaluated_tasks_that_are_not_a_list_are_invalid_data() {
    let (_tmp, path) = file_with("tasks.json", "{}");
    assert_eq!(
        load_init_evaluated_tasks(&path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_record_without_nodes_is_invalid_data() {
    assert_eq!(
        tree_from_record(&json!({})).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_node_without_a_commit_is_invalid_data() {
    assert_eq!(
        tree_from_record(&json!({"nodes": [{"id": 1}]}))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_node_without_an_id_is_invalid_data() {
    assert_eq!(
        tree_from_record(&json!({"nodes": [{"commit_id": "c"}]}))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn node_metadata_without_performance_is_invalid_data() {
    let (tmp, _) = file_with("metadata.json", "{}");
    assert_eq!(
        read_overall_performance(tmp.path()).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn node_metadata_that_is_not_json_is_invalid_data() {
    let (tmp, _) = file_with("metadata.json", "not json");
    assert_eq!(
        read_overall_performance(tmp.path()).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn accuracy_without_submissions_is_zero() {
    assert_eq!(OverallPerformance::default().accuracy_score(), 0.0);
}

#[test]
fn accuracy_is_resolved_over_submitted() {
    let perf = OverallPerformance {
        total_resolved_instances: 1,
        total_submitted_instances: 4,
        ..OverallPerformance::default()
    };
    assert_eq!(perf.accuracy_score(), 0.25);
}

fn results_dir() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("run_a.json"),
        r#"{"file": "other", "accuracy_score": 0.5, "resolved_instances": 1, "submitted_instances": 3}"#,
    )
    .unwrap();
    fs::write(tmp.path().join("run_b.json"), "[]").unwrap();
    fs::write(tmp.path().join("run_c.txt"), "{}").unwrap();
    tmp
}

fn results() -> (Vec<PyValue>, OverallPerformance) {
    get_all_performance(results_dir().path(), "run").unwrap()
}

#[test]
fn results_files_are_the_json_files_of_the_run() {
    assert_eq!(results().1.files, ["run_a.json", "run_b.json"]);
}

#[test]
fn results_files_add_up_their_submissions() {
    assert_eq!(results().1.total_submitted_instances, 3);
}

#[test]
fn a_results_file_overrides_the_file_name() {
    assert_eq!(py(&results().0[0])["file"], "other");
}

#[test]
fn a_results_file_overrides_the_accuracy() {
    assert_eq!(py(&results().0[0])["accuracy_score"], 0.5);
}

#[test]
fn a_results_file_that_is_a_list_scores_zero() {
    assert_eq!(py(&results().0[1])["accuracy_score"], 0);
}

#[test]
fn a_broken_results_file_is_an_error() {
    let tmp = results_dir();
    fs::write(tmp.path().join("run_d.json"), "{").unwrap();
    assert!(get_all_performance(tmp.path(), "run").is_err());
}

#[test]
fn performance_of_a_missing_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(get_all_performance(&tmp.path().join("missing"), "run").is_err());
}

fn missing() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("missing");
    (tmp, path)
}

#[test]
fn loading_missing_metadata_is_an_error() {
    let (_tmp, path) = missing();
    assert!(load_rustyhgm_metadata(&path).is_err());
}

#[test]
fn loading_the_last_record_of_missing_metadata_is_an_error() {
    let (_tmp, path) = missing();
    assert!(load_last_rustyhgm_metadata(&path).is_err());
}

#[test]
fn loading_missing_init_evaluated_tasks_is_an_error() {
    let (_tmp, path) = missing();
    assert!(load_init_evaluated_tasks(&path).is_err());
}

#[test]
fn reading_performance_of_a_missing_node_is_an_error() {
    let (_tmp, path) = missing();
    assert!(read_overall_performance(&path).is_err());
}

#[test]
fn updating_metadata_in_a_missing_directory_is_an_error() {
    let (_tmp, path) = missing();
    assert!(update_metadata(&path, "r", &Tree::with_root(), 0, &[]).is_err());
}

#[test]
fn a_results_entry_that_is_a_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("run_dir.json")).unwrap();
    assert!(get_all_performance(tmp.path(), "run").is_err());
}

#[test]
fn patch_paths_of_a_missing_node_are_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(model_patch_paths(tmp.path(), "h", "c").is_err());
}

#[test]
fn patch_paths_through_broken_metadata_are_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("h");
    fs::create_dir_all(run.join("c")).unwrap();
    fs::write(run.join("c/metadata.json"), "{").unwrap();
    assert!(model_patch_paths(tmp.path(), "h", "c").is_err());
}

#[test]
fn a_record_gives_back_its_run_id() {
    assert_eq!(run_id_from_record(&json!({"run_id": "r"})).unwrap(), "r");
}

#[test]
fn a_record_without_a_run_id_is_invalid() {
    assert_eq!(
        run_id_from_record(&json!({"nodes": []}))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}
