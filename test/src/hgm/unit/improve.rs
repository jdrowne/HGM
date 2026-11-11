use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use hgm::FAILED;
use hgm::improve::{Improver, SelfImprove, child_performance, choose_entry, diagnose_problem};
use hgm_agent::llm::{Client, Provider};
use hgm_core::log::Log;
use hgm_core::prompts::{
    CONTEXT_LENGTH_ERRORS, DiagnoseInput, SOLVE_CONTEXTLENGTH, SOLVE_EMPTY_PATCHES,
    SOLVE_STOCHASTICITY,
};
use hgm_core::pyjson::dumps;
use hgm_sandbox::Docker;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde_json::{Value, json};

use crate::common::{DIAGNOSIS, node, performance, read_json, reply, serve};

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

#[test]
fn polyglot_entries_prefer_unsolved_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    node(out, "a", None, performance(&["r"], &["u"], &["e"]));
    assert_eq!(outcomes(out, "a", true), set(&["e", "u"]));
    node(out, "b", None, performance(&["r"], &[], &[]));
    assert_eq!(outcomes(out, "b", true), set(&["r"]));
    node(out, "c", None, performance(&[], &[], &[]));
    assert_eq!(
        outcomes(out, "c", true),
        set(&["error: Failed to choose an entry for self-improvement based on c."])
    );
    let e = choose_entry(out, "missing", true, &mut StdRng::seed_from_u64(0)).unwrap_err();
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
    node(out, "b", None, performance(&["r"], &[], &[]));
    fs::create_dir_all(out.join("b/predictions")).unwrap();
    assert_eq!(outcomes(out, "b", false), set(&[SOLVE_STOCHASTICITY, "r"]));
    node(out, "c", None, performance(&[], &[], &[]));
    assert_eq!(
        outcomes(out, "c", false),
        set(&[
            SOLVE_EMPTY_PATCHES,
            SOLVE_STOCHASTICITY,
            "error: Failed to choose an entry for self-improvement based on c."
        ])
    );
    node(out, "d", None, performance(&["r"], &["u"], &[]));
    let found = outcomes(out, "d", false);
    assert!(found.contains(SOLVE_STOCHASTICITY), "{found:?}");
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

#[test]
fn a_diagnosis_is_retried_until_it_parses() {
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
    assert!(statement.contains("broken"), "{statement}");
    assert!(statement.contains("do it"), "{statement}");
    assert_eq!(seen.lock().unwrap().len(), 3);
    let text = fs::read_to_string(&log_path).unwrap();
    assert!(
        text.contains("Error with get_response_from_llm: "),
        "{text}"
    );
    assert!(
        text.contains("Error while diagnosing the problem: empty response json"),
        "{text}"
    );
}

#[test]
fn a_diagnosis_gives_up_after_three_attempts() {
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
    assert_eq!(statement, None);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn a_diagnosis_prompt_that_cannot_be_built_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let unused = client("http://127.0.0.1:9/v1".into());
    let mut rng = StdRng::seed_from_u64(0);
    assert!(
        diagnose_problem(
            &input("task", tmp.path(), &[]),
            false,
            &unused,
            &mut rng,
            &Log::discard()
        )
        .is_err()
    );
    assert!(
        diagnose_problem(
            &input("task", tmp.path(), &[]),
            true,
            &unused,
            &mut rng,
            &Log::discard()
        )
        .is_err()
    );
}

#[test]
fn polyglot_diagnoses_read_the_task_logs() {
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
    assert!(statement.unwrap().contains("broken"));
    assert_eq!(seen.lock().unwrap().len(), 1);
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
        root_dir: out.to_path_buf(),
        output_dir: out.to_path_buf(),
        polyglot: false,
        dataset,
        image_name: "hgm-no-such-image:latest".into(),
        diagnose: client("http://127.0.0.1:9/v1".into()),
        self_improve_llm: "m".into(),
        timeout: 1,
    }
}

fn only_run_dir(out: &Path) -> PathBuf {
    let dirs: Vec<PathBuf> = fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("self_improve.log").exists())
        .collect();
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    dirs[0].clone()
}

#[test]
fn a_child_fails_when_its_dataset_or_docker_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("a");
    fs::create_dir_all(&out).unwrap();
    let no_data = improver(
        &out,
        Docker::new("hgm-no-such-docker-program"),
        Err("no dataset".into()),
    );
    assert_eq!(no_data.sample_child("p"), FAILED);
    let run_dir = only_run_dir(&out);
    let metadata = read_json(&run_dir.join("metadata.json"));
    assert_eq!(metadata["parent_commit"], "p");
    assert_eq!(
        metadata["run_id"],
        run_dir.file_name().unwrap().to_str().unwrap()
    );
    assert!(
        fs::read_to_string(run_dir.join("self_improve.log"))
            .unwrap()
            .contains("Error while sampling a child: no dataset")
    );
    let out = tmp.path().join("b");
    fs::create_dir_all(&out).unwrap();
    let no_docker = improver(
        &out,
        Docker::new("hgm-no-such-docker-program"),
        Ok(Vec::new()),
    );
    assert_eq!(no_docker.sample_child("p"), FAILED);
    assert!(only_run_dir(&out).join("metadata.json").is_file());
}

#[test]
fn a_child_fails_when_its_run_directory_cannot_be_made() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    let blocked = improver(
        &file,
        Docker::new("hgm-no-such-docker-program"),
        Ok(Vec::new()),
    );
    assert_eq!(blocked.sample_child("p"), FAILED);
}

#[cfg(unix)]
#[test]
fn a_child_whose_metadata_cannot_be_saved_still_fails_cleanly() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    let docker = out.join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in hgm-container-*) mkdir -p \"$(dirname \"$0\")/${a#hgm-container-}/metadata.json\" ;; esac; done\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let improver = improver(out, Docker::new(docker.to_string_lossy()), Ok(Vec::new()));
    assert_eq!(improver.sample_child("p"), FAILED);
    let log = fs::read_to_string(only_run_dir(out).join("self_improve.log")).unwrap();
    assert!(log.contains("Error saving metadata: "), "{log}");
}
