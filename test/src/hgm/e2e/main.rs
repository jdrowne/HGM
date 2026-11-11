use std::path::PathBuf;
use std::process::Command;

mod improve;
mod prepare;

fn binary(name: &str) -> PathBuf {
    std::env::var_os(format!("NEXTEST_BIN_EXE_{name}"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_BIN_EXE_hgm"))
                .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        })
}

#[test]
fn hgm_binary_runs() {
    let out = Command::new(binary("hgm"))
        .arg("--version")
        .output()
        .expect("run hgm");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("hgm "), "{stdout}");
}

#[test]
fn hgm_with_an_unknown_argument_exits_with_usage_status() {
    for arg in ["--no_such_flag", "run"] {
        let out = Command::new(binary("hgm"))
            .arg(arg)
            .output()
            .expect("run hgm");
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8(out.stderr).unwrap().contains(arg));
    }
}

fn hgm_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(binary("hgm"));
    cmd.current_dir(dir)
        .env("HGM_DATA_DIR", dir.join("cache"))
        .env("HGM_HF_ENDPOINT", "http://127.0.0.1:9")
        .env("HGM_DOCKER", "hgm-no-such-docker-program");
    cmd
}

fn seed_source(root: &std::path::Path) {
    let src = root.join("initial_swe/agent/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "seed\n").unwrap();
}

const CHILD_PATCH: &str = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n seed\n+child\n";

fn metadata(dir: &std::path::Path, commit: &str, extra: serde_json::Value) {
    let mut value = serde_json::json!({
        "run_id": commit,
        "overall_performance": {
            "accuracy_score": 0.5,
            "total_resolved_instances": 1,
            "total_submitted_instances": 2,
            "files": [],
            "total_unresolved_ids": ["b"],
            "total_emptypatch_ids": [],
            "total_resolved_ids": ["a"],
            "total_submitted_ids": ["a", "b"],
        },
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let node = dir.join(commit);
    std::fs::create_dir_all(&node).unwrap();
    std::fs::write(node.join("metadata.json"), value.to_string()).unwrap();
}

fn finished_run(root: &std::path::Path) {
    metadata(&root.join("initial_swe"), "agent", serde_json::json!({}));
    seed_source(root);
    let prev = root.join("output_hgm/prev");
    metadata(&prev, "c1", serde_json::json!({"parent_commit": "initial"}));
    std::fs::write(prev.join("c1/model_patch.diff"), CHILD_PATCH).unwrap();
    std::fs::write(prev.join("init_evaluated_tasks.json"), "[]").unwrap();
    std::fs::write(
        prev.join("hgm_metadata.jsonl"),
        "{\"n_task_evals\": 2, \"nodes\": [{\"commit_id\": \"c1\", \"id\": 1, \"parent_id\": 0, \"mean_utility\": 0.5, \"num_evals\": 2}]}\n",
    )
    .unwrap();
}

const FINISHED: [&str; 8] = [
    "--initial_agent_name",
    "agent",
    "--continue_from",
    "output_hgm/prev",
    "--max_task_evals",
    "2",
    "--max_workers",
    "1",
];

#[test]
fn a_finished_run_can_be_continued_without_more_work() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    finished_run(root);
    let out = hgm_in(root)
        .args([
            "--initial_agent_name",
            "agent",
            "--continue_from",
            "output_hgm/prev",
            "--max_task_evals",
            "2",
            "--max_workers",
            "1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("Output directory: "), "{stdout}");
    assert!(
        stdout.contains("Best agent: c1 solved 1/2 (score 0."),
        "{stdout}"
    );
    let output = root.join("output_hgm/prev");
    assert!(
        stdout.contains(&format!(
            "{}\n",
            std::path::Path::new("output_hgm")
                .join("prev")
                .join("best_agent")
                .display()
        )),
        "{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("best_agent/lib.rs")).unwrap(),
        "seed\nchild\n"
    );
    assert!(output.join("initial/metadata.json").is_file());
    let log = std::fs::read_to_string(output.join("hgm_outer.log")).unwrap();
    assert!(
        log.contains("Starting HGM run prev with configuration: {"),
        "{log}"
    );
    assert!(log.contains("\"max_task_evals\":2"), "{log}");
}

#[test]
fn a_run_without_an_agent_name_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let out = hgm_in(tmp.path())
        .args(["--initial_agent_name", ""])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("\nhgm: Initial agent name must be provided")
    );
}

#[cfg(unix)]
fn swe_cache(root: &std::path::Path) -> PathBuf {
    for split in ["SWE-bench_Lite", "SWE-bench"] {
        let cache = root.join("cache/princeton-nlp").join(split).join("test");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/src/bench/unit/fixtures/rows.parquet"),
            cache.join("rows.parquet"),
        )
        .unwrap();
        std::fs::write(cache.join("files.json"), r#"["rows.parquet"]"#).unwrap();
    }
    let python = root.join("python");
    std::fs::write(
        &python,
        "#!/bin/sh\n[ \"$1\" = -m ] && exit 0\ncat > /dev/null\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o755)).unwrap();
    python
}

#[cfg(unix)]
#[test]
fn a_holdout_split_scores_the_best_agent_after_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    finished_run(root);
    let python = swe_cache(root);
    let holdout = root.join("output_hgm/prev/holdout/best_agent");
    std::fs::create_dir_all(&holdout).unwrap();
    std::fs::write(holdout.join("hgm-agent"), "").unwrap();
    let out = hgm_in(root)
        .env("HGM_SWEBENCH_PYTHON", &python)
        .args(FINISHED)
        .args(["--holdout_split", "Lite", "--holdout_tasks", "2"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("Held-out Lite: solved 0/2\n"), "{stdout}");
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(holdout.join("metadata.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["run_id"], "best_agent");
    assert!(metadata["swe_dnames"].is_array(), "{metadata}");
}

#[test]
fn a_holdout_split_needs_its_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    finished_run(root);
    let out = hgm_in(root)
        .args(FINISHED)
        .args(["--holdout_split", "Lite"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("\nhgm: "));
    assert!(root.join("output_hgm/prev/best_agent/lib.rs").is_file());
}

fn continued(root: &std::path::Path, nodes: &[&str], n_task_evals: usize) {
    let prev = root.join("output_hgm/prev");
    metadata(&root.join("initial_swe"), "agent", serde_json::json!({}));
    let mut records = Vec::new();
    for (i, commit) in nodes.iter().enumerate() {
        metadata(
            &prev,
            commit,
            serde_json::json!({"parent_commit": "initial"}),
        );
        records.push(serde_json::json!({"commit_id": commit, "id": i + 1, "parent_id": 0}));
    }
    std::fs::write(prev.join("init_evaluated_tasks.json"), "[]").unwrap();
    std::fs::write(
        prev.join("hgm_metadata.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"n_task_evals": n_task_evals, "nodes": records})
        ),
    )
    .unwrap();
}

#[test]
fn a_continued_run_evaluates_until_the_budget_is_spent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    continued(root, &["c1", "c2", "c3"], 1);
    let out = hgm_in(root)
        .args([
            "--initial_agent_name",
            "agent",
            "--continue_from",
            "output_hgm/prev",
            "--max_task_evals",
            "7",
            "--max_workers",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let log = std::fs::read_to_string(root.join("output_hgm/prev/hgm_outer.log")).unwrap();
    assert!(log.contains("Error: "), "{log}");
}

#[test]
fn runs_fail_when_their_tasks_or_records_are_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = hgm_in(root)
        .args(["--full_eval", "--output_dir", "full"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("hgm: downloading the dataset"));
    let out = hgm_in(root)
        .args(["--continue_from", "output_hgm/none"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("init_evaluated_tasks.json"));
}

#[test]
fn hgm_needs_a_working_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg("cd \"$1\" && rmdir \"$1\" && exec \"$2\"")
        .arg("sh")
        .arg(&gone)
        .arg(binary("hgm"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("hgm: "));
}
