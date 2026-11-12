use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rand::SeedableRng;
use rand::rngs::StdRng;
use rustyhgm::FAILED;
use rustyhgm::improve::{
    Improver, SelfImprove, child_performance, choose_entry, claim_run_dir, container_name,
    diagnose_problem,
};
use rustyhgm_coding_harness::llm::{Client, Provider};
use rustyhgm_core::log::Log;
use rustyhgm_core::prompts::{
    CONTEXT_LENGTH_ERRORS, DiagnoseInput, SOLVE_CONTEXTLENGTH, SOLVE_EMPTY_PATCHES,
    SOLVE_STOCHASTICITY,
};
use rustyhgm_core::pyjson::dumps;
use rustyhgm_sandbox::Docker;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::common::{DIAGNOSIS, node, performance, read_json, reply, serve, service};

fn outcomes(out: &Path, commit: &str, polyglot: bool) -> BTreeSet<String> {
    (0..400)
        .map(|seed| {
            choose_entry(out, commit, polyglot, &mut StdRng::seed_from_u64(seed))
                .unwrap_or_else(|e| format!("error: {e}"))
        })
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn context_error_log(out: &Path, commit: &str, id: &str) {
    let dir = out.join(commit).join("predictions/run");
    fs::create_dir_all(&dir).unwrap();
    let line = CONTEXT_LENGTH_ERRORS[0].replace('\\', "");
    fs::write(dir.join(format!("{id}.md")), format!("{line}\n{line}\n")).unwrap();
}

fn entries(commit: &str, perf: Value, polyglot: bool) -> BTreeSet<String> {
    let tmp = tempfile::tempdir().unwrap();
    node(tmp.path(), commit, None, perf);
    outcomes(tmp.path(), commit, polyglot)
}

#[test]
fn polyglot_entries_prefer_unsolved_tasks() {
    assert_eq!(
        entries("a", performance(&["r"], &["u"], &["e"]), true),
        set(&["e", "u"])
    );
}

#[test]
fn polyglot_entries_fall_back_to_solved_tasks() {
    assert_eq!(
        entries("b", performance(&["r"], &[], &[]), true),
        set(&["r"])
    );
}

#[test]
fn polyglot_entries_fail_without_any_tasks() {
    assert_eq!(
        entries("c", performance(&[], &[], &[]), true),
        set(&["error: Failed to choose an entry for self-improvement based on c."])
    );
}

#[test]
fn a_missing_polyglot_parent_is_not_eligible() {
    let tmp = tempfile::tempdir().unwrap();
    let e = choose_entry(tmp.path(), "missing", true, &mut StdRng::seed_from_u64(0)).unwrap_err();
    assert!(
        e.starts_with("missing not eligible for being a parent: "),
        "{e}"
    );
}

#[test]
fn swe_entries_mix_special_prompts_with_unresolved_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    node(out, "a", None, performance(&["r"], &["u"], &["e"]));
    context_error_log(out, "a", "e");
    assert_eq!(
        outcomes(out, "a", false),
        set(&[
            SOLVE_EMPTY_PATCHES,
            SOLVE_STOCHASTICITY,
            SOLVE_CONTEXTLENGTH,
            "u"
        ])
    );
}

#[test]
fn swe_entries_of_a_fully_solved_parent_are_stochasticity_or_solved_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    node(out, "b", None, performance(&["r"], &[], &[]));
    fs::create_dir_all(out.join("b/predictions")).unwrap();
    assert_eq!(outcomes(out, "b", false), set(&[SOLVE_STOCHASTICITY, "r"]));
}

#[test]
fn swe_entries_of_a_parent_without_tasks_keep_the_special_prompts() {
    assert_eq!(
        entries("c", performance(&[], &[], &[]), false),
        set(&[
            SOLVE_EMPTY_PATCHES,
            SOLVE_STOCHASTICITY,
            "error: Failed to choose an entry for self-improvement based on c."
        ])
    );
}

fn entries_without_predictions() -> BTreeSet<String> {
    entries("d", performance(&["r"], &["u"], &[]), false)
}

#[test]
fn swe_entries_without_predictions_can_pick_stochasticity() {
    let found = entries_without_predictions();
    assert!(found.contains(SOLVE_STOCHASTICITY), "{found:?}");
}

#[test]
fn swe_entries_without_predictions_can_fail() {
    let found = entries_without_predictions();
    assert!(found.iter().any(|o| o.starts_with("error: ")), "{found:?}");
}

fn client(base_url: String) -> Client {
    Client {
        provider: Provider::OpenRouter,
        model: "m".into(),
        base_url,
        api_key: None,
    }
}

fn input<'a>(entry_id: &'a str, root: &'a Path, dataset: &'a [Value]) -> DiagnoseInput<'a> {
    DiagnoseInput {
        entry_id,
        commit: "c",
        root_dir: root,
        out_dir: root,
        dataset,
        patch_files: &[],
    }
}

struct Retried {
    statement: String,
    requests: usize,
    log: String,
}

fn retried_diagnosis() -> Retried {
    let tmp = tempfile::tempdir().unwrap();
    let log_path = tmp.path().join("log");
    let log = Log::to_file(&log_path).unwrap();
    let (base, seen) = serve(vec![
        (500, "{}".to_owned()),
        reply("no json here"),
        reply(DIAGNOSIS),
    ]);
    let statement = diagnose_problem(
        &input(SOLVE_STOCHASTICITY, tmp.path(), &[]),
        false,
        &client(base),
        &mut StdRng::seed_from_u64(0),
        &log,
    )
    .unwrap()
    .unwrap();
    let requests = seen.lock().unwrap().len();
    Retried {
        statement,
        requests,
        log: fs::read_to_string(&log_path).unwrap(),
    }
}

#[test]
fn a_retried_diagnosis_states_the_problem() {
    let statement = retried_diagnosis().statement;
    assert!(statement.contains("broken"), "{statement}");
}

#[test]
fn a_retried_diagnosis_states_the_suggestion() {
    let statement = retried_diagnosis().statement;
    assert!(statement.contains("do it"), "{statement}");
}

#[test]
fn a_diagnosis_is_retried_until_it_parses() {
    assert_eq!(retried_diagnosis().requests, 3);
}

#[test]
fn a_failed_diagnosis_request_is_logged() {
    let text = retried_diagnosis().log;
    assert!(
        text.contains("Error with get_response_from_llm: "),
        "{text}"
    );
}

#[test]
fn an_unparsable_diagnosis_is_logged() {
    let text = retried_diagnosis().log;
    assert!(
        text.contains("Error while diagnosing the problem: empty response json"),
        "{text}"
    );
}

fn abandoned_diagnosis() -> (Option<String>, usize) {
    let tmp = tempfile::tempdir().unwrap();
    let (base, seen) = serve(vec![
        reply("```json\n{\"problem_description\": \"x\"}\n```"),
        reply("nothing"),
        reply("nothing"),
        reply(DIAGNOSIS),
    ]);
    let statement = diagnose_problem(
        &input(SOLVE_EMPTY_PATCHES, tmp.path(), &[]),
        false,
        &client(base),
        &mut StdRng::seed_from_u64(0),
        &Log::discard(),
    )
    .unwrap();
    let requests = seen.lock().unwrap().len();
    (statement, requests)
}

#[test]
fn a_diagnosis_gives_up_with_no_statement() {
    assert_eq!(abandoned_diagnosis().0, None);
}

#[test]
fn a_diagnosis_gives_up_after_three_attempts() {
    assert_eq!(abandoned_diagnosis().1, 3);
}

fn unbuildable_diagnosis(polyglot: bool) -> Result<Option<String>, String> {
    let tmp = tempfile::tempdir().unwrap();
    diagnose_problem(
        &input("task", tmp.path(), &[]),
        polyglot,
        &client("http://127.0.0.1:9/v1".into()),
        &mut StdRng::seed_from_u64(0),
        &Log::discard(),
    )
}

#[test]
fn a_swe_diagnosis_prompt_that_cannot_be_built_is_an_error() {
    assert!(unbuildable_diagnosis(false).is_err());
}

#[test]
fn a_polyglot_diagnosis_prompt_that_cannot_be_built_is_an_error() {
    assert!(unbuildable_diagnosis(true).is_err());
}

fn polyglot_diagnosis() -> (Option<String>, usize) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("c/predictions/c_0")).unwrap();
    let dataset = [json!({
        "instance_id": "python__x",
        "language": "python",
        "problem_statement": "p",
        "reference_tests": "t",
    })];
    let (base, seen) = serve(vec![reply(DIAGNOSIS)]);
    let statement = diagnose_problem(
        &input("python__x", root, &dataset),
        true,
        &client(base),
        &mut StdRng::seed_from_u64(0),
        &Log::discard(),
    )
    .unwrap();
    let requests = seen.lock().unwrap().len();
    (statement, requests)
}

#[test]
fn polyglot_diagnoses_read_the_task_logs() {
    assert!(polyglot_diagnosis().0.unwrap().contains("broken"));
}

#[test]
fn a_polyglot_diagnosis_takes_one_request() {
    assert_eq!(polyglot_diagnosis().1, 1);
}

#[test]
fn a_child_starts_with_zeroed_performance() {
    assert_eq!(
        dumps(&child_performance(), None),
        "{\"accuracy_score\": 0.0, \"total_resolved_instances\": 0, \"total_submitted_instances\": 0, \"files\": [], \"total_submitted_ids\": [], \"total_unresolved_ids\": [], \"total_emptypatch_ids\": [], \"total_resolved_ids\": []}"
    );
}

fn improver(out: &Path, docker: Docker, dataset: Result<Vec<Value>, String>) -> SelfImprove {
    SelfImprove {
        docker,
        harness_source: out.to_path_buf(),
        output_dir: out.to_path_buf(),
        polyglot: false,
        dataset,
        image_name: "rustyhgm-no-such-image:latest".into(),
        diagnose: client("http://127.0.0.1:9/v1".into()),
        self_improve_llm: "m".into(),
        self_improve_service: service(),
        timeout: 1,
    }
}

fn run_dirs(out: &Path) -> Vec<PathBuf> {
    fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("self_improve.log").exists())
        .collect()
}

fn only_run_dir(out: &Path) -> PathBuf {
    let [dir]: [PathBuf; 1] = run_dirs(out).try_into().unwrap();
    dir
}

fn child_without_data() -> (TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("a");
    fs::create_dir_all(&out).unwrap();
    let no_data = improver(
        &out,
        Docker::new("rustyhgm-no-such-docker-program"),
        Err("no dataset".into()),
    );
    let commit = no_data.sample_child("p");
    (tmp, out, commit)
}

#[test]
fn a_child_fails_when_its_dataset_is_missing() {
    assert_eq!(child_without_data().2, FAILED);
}

#[test]
fn a_failed_child_leaves_one_run_directory() {
    let (_tmp, out, _) = child_without_data();
    assert_eq!(run_dirs(&out).len(), 1);
}

#[test]
fn a_failed_child_records_its_parent() {
    let (_tmp, out, _) = child_without_data();
    let metadata = read_json(&only_run_dir(&out).join("metadata.json"));
    assert_eq!(metadata["parent_commit"], "p");
}

#[test]
fn a_failed_child_records_its_run_id() {
    let (_tmp, out, _) = child_without_data();
    let run_dir = only_run_dir(&out);
    let metadata = read_json(&run_dir.join("metadata.json"));
    assert_eq!(
        metadata["run_id"],
        run_dir.file_name().unwrap().to_str().unwrap()
    );
}

#[test]
fn a_child_without_data_logs_the_dataset_error() {
    let (_tmp, out, _) = child_without_data();
    assert!(
        fs::read_to_string(only_run_dir(&out).join("self_improve.log"))
            .unwrap()
            .contains("Error while sampling a child: no dataset")
    );
}

fn child_without_docker() -> (TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("b");
    fs::create_dir_all(&out).unwrap();
    let no_docker = improver(
        &out,
        Docker::new("rustyhgm-no-such-docker-program"),
        Ok(Vec::new()),
    );
    let commit = no_docker.sample_child("p");
    (tmp, out, commit)
}

#[test]
fn a_child_fails_when_docker_is_missing() {
    assert_eq!(child_without_docker().2, FAILED);
}

#[test]
fn a_child_without_docker_still_saves_its_metadata() {
    let (_tmp, out, _) = child_without_docker();
    assert!(only_run_dir(&out).join("metadata.json").is_file());
}

#[test]
fn a_child_fails_when_its_run_directory_cannot_be_made() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    let blocked = improver(
        &file,
        Docker::new("rustyhgm-no-such-docker-program"),
        Ok(Vec::new()),
    );
    assert_eq!(blocked.sample_child("p"), FAILED);
}

#[cfg(unix)]
fn child_with_unsavable_metadata() -> (TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    let docker = out.join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in rustyhgm-container-*) id=\"${a#rustyhgm-container-}\"; mkdir -p \"$(dirname \"$0\")/${id%-*}/metadata.json\" ;; esac; done\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let improver = improver(out, Docker::new(docker.to_string_lossy()), Ok(Vec::new()));
    let commit = improver.sample_child("p");
    (tmp, commit)
}

#[cfg(unix)]
#[test]
fn a_child_whose_metadata_cannot_be_saved_still_fails_cleanly() {
    assert_eq!(child_with_unsavable_metadata().1, FAILED);
}

#[cfg(unix)]
#[test]
fn a_child_whose_metadata_cannot_be_saved_logs_the_error() {
    let (tmp, _) = child_with_unsavable_metadata();
    let log = fs::read_to_string(only_run_dir(tmp.path()).join("self_improve.log")).unwrap();
    assert!(log.contains("Error saving metadata: "), "{log}");
}

fn claimed_concurrently(out: &Path) -> BTreeSet<String> {
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..16)
            .map(|_| s.spawn(|| claim_run_dir(out).unwrap().0))
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    })
}

#[test]
fn run_folders_claimed_at_the_same_time_are_all_different() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(claimed_concurrently(tmp.path()).len(), 16);
}

#[test]
fn a_claimed_run_folder_is_made_inside_a_missing_output_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("missing");
    assert!(claim_run_dir(&out).unwrap().1.starts_with(&out));
}

#[test]
fn a_claimed_run_folder_is_named_for_its_run_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (id, dir) = claim_run_dir(tmp.path()).unwrap();
    assert_eq!(dir, tmp.path().join(id));
}

#[test]
fn a_run_folder_cannot_be_claimed_under_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    assert!(claim_run_dir(&file).is_err());
}

#[test]
fn containers_for_the_same_run_get_different_names() {
    assert_ne!(container_name("r1"), container_name("r1"));
}

#[test]
fn a_container_name_starts_with_its_run_id() {
    assert!(container_name("r1").starts_with("rustyhgm-container-r1-"));
}
