use std::path::PathBuf;
use std::process::Command;

mod improve;
mod prepare;

fn binary(name: &str) -> PathBuf {
    std::env::var_os(format!("NEXTEST_BIN_EXE_{name}"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_BIN_EXE_rustyhgm"))
                .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        })
}

const SETTINGS: [(&str, &str); 27] = [
    ("--self_improve_llm", "m"),
    ("--self_improve_provider", "openrouter"),
    ("--self_improve_base_url", "http://127.0.0.1:9/v1"),
    ("--downstream_llm", "m"),
    ("--downstream_provider", "openrouter"),
    ("--downstream_base_url", "http://127.0.0.1:9/v1"),
    ("--diagnose_llm", "m"),
    ("--diagnose_provider", "openrouter"),
    ("--diagnose_base_url", "http://127.0.0.1:9/v1"),
    ("--api_key_file", "key"),
    ("--docker", "rustyhgm-no-such-docker-program"),
    ("--swebench_python", "rustyhgm-no-such-python"),
    ("--dataset_endpoint", "http://127.0.0.1:9"),
    ("--dataset_dir", "cache"),
    ("--polyglot_repo", "no-such-repo"),
    ("--polyglot_dir", "polyglot"),
    ("--alpha", "0.6"),
    ("--beta", "1.0"),
    ("--eval_random_level", "1.0"),
    ("--n_pseudo_descendant_evals", "10000"),
    ("--expansion_statistic", "clade"),
    ("--epsilon", "0.01"),
    ("--max_workers", "1"),
    ("--max_task_evals", "800"),
    ("--self_improve_timeout", "3600"),
    ("--evaluation_timeout", "3600"),
    ("--output_dir", "out"),
];

fn flags(changes: &[(&str, &str)]) -> Vec<String> {
    let set = SETTINGS.iter().map(|(flag, value)| {
        let changed = changes.iter().find(|(f, _)| f == flag);
        vec![*flag, changed.map_or(*value, |(_, v)| *v)]
    });
    let extra = changes
        .iter()
        .filter(|(flag, _)| SETTINGS.iter().all(|(f, _)| f != flag))
        .map(|(flag, value)| {
            [*flag, *value]
                .into_iter()
                .filter(|w| !w.is_empty())
                .collect()
        });
    set.chain(extra).flatten().map(str::to_owned).collect()
}

struct Ran {
    tmp: tempfile::TempDir,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn root(&self) -> &std::path::Path {
        self.tmp.path()
    }

    fn file(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path)).unwrap()
    }
}

fn ran(tmp: tempfile::TempDir, out: std::process::Output) -> Ran {
    Ran {
        tmp,
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn rustyhgm_with(args: &[&str]) -> Ran {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(binary("rustyhgm"))
        .args(args)
        .output()
        .expect("run rustyhgm");
    ran(tmp, out)
}

#[test]
fn rustyhgm_binary_runs() {
    assert_eq!(rustyhgm_with(&["--version"]).code, Some(0));
}

#[test]
fn rustyhgm_prints_its_name_and_version() {
    let ran = rustyhgm_with(&["--version"]);
    assert!(ran.stdout.starts_with("rustyhgm "), "{}", ran.stdout);
}

#[test]
fn rustyhgm_with_an_unknown_flag_exits_with_usage_status() {
    assert_eq!(rustyhgm_with(&["--no_such_flag"]).code, Some(2));
}

#[test]
fn rustyhgm_with_an_unknown_flag_names_it() {
    assert!(
        rustyhgm_with(&["--no_such_flag"])
            .stderr
            .contains("--no_such_flag")
    );
}

fn stray() -> Ran {
    let mut args = flags(&[]);
    args.push("run".to_owned());
    rustyhgm_with(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn rustyhgm_with_a_stray_argument_exits_with_usage_status() {
    assert_eq!(stray().code, Some(2));
}

#[test]
fn rustyhgm_with_a_stray_argument_names_it() {
    assert!(stray().stderr.contains("'run'"));
}

#[test]
fn rustyhgm_without_its_flags_exits_with_usage_status() {
    assert_eq!(rustyhgm_with(&[]).code, Some(2));
}

#[test]
fn rustyhgm_without_its_flags_names_them() {
    assert!(rustyhgm_with(&[]).stderr.contains("--output_dir <DIR>"));
}

fn key_file(dir: &std::path::Path) {
    std::fs::write(dir.join("key"), "k\n").unwrap();
}

fn rustyhgm_in(dir: &std::path::Path) -> Command {
    key_file(dir);
    let mut cmd = Command::new(binary("rustyhgm"));
    cmd.current_dir(dir);
    cmd
}

const RUN_ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

fn progenitor_source(root: &std::path::Path) {
    let src = root.join("output_rustyhgm/prev/progenitor/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "progenitor\n").unwrap();
}

const CHILD_PATCH: &str = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n progenitor\n+child\n";

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
    let prev = root.join("output_rustyhgm/prev");
    metadata(&prev, "progenitor", serde_json::json!({}));
    progenitor_source(root);
    metadata(
        &prev,
        "c1",
        serde_json::json!({"parent_commit": "progenitor"}),
    );
    std::fs::write(prev.join("c1/model_patch.diff"), CHILD_PATCH).unwrap();
    std::fs::write(prev.join("init_evaluated_tasks.json"), "[]").unwrap();
    std::fs::write(
        prev.join("rustyhgm_metadata.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"run_id": RUN_ID, "n_task_evals": 2, "nodes": [{"commit_id": "c1", "id": 1, "parent_id": 0, "mean_utility": 0.5, "num_evals": 2}]})
        ),
    )
    .unwrap();
}

const FINISHED: [(&str, &str); 3] = [
    ("--continue_from", "output_rustyhgm/prev"),
    ("--output_dir", "output_rustyhgm/prev"),
    ("--max_task_evals", "2"),
];

fn in_root(prepare: impl FnOnce(&std::path::Path), cmd: impl FnOnce(&mut Command)) -> Ran {
    let tmp = tempfile::tempdir().unwrap();
    prepare(tmp.path());
    let mut command = rustyhgm_in(tmp.path());
    cmd(&mut command);
    let out = command.output().unwrap();
    ran(tmp, out)
}

fn continued_finished() -> Ran {
    in_root(finished_run, |c| {
        c.args(flags(&FINISHED));
    })
}

#[test]
fn a_finished_run_can_be_continued_without_more_work() {
    let ran = continued_finished();
    assert_eq!(ran.code, Some(0), "{}{}", ran.stdout, ran.stderr);
}

#[test]
fn a_continued_run_prints_its_output_directory() {
    assert!(continued_finished().stdout.contains("Output directory: "));
}

#[test]
fn a_continued_run_prints_the_best_harness() {
    assert!(
        continued_finished()
            .stdout
            .contains("Best harness: c1 solved 1/2 (score 0.")
    );
}

#[test]
fn a_continued_run_prints_where_the_best_harness_is() {
    let best = std::path::Path::new("output_rustyhgm/prev").join("best_harness");
    assert!(
        continued_finished()
            .stdout
            .contains(&format!("{}\n", best.display()))
    );
}

#[test]
fn a_continued_run_writes_the_best_harness_source() {
    assert_eq!(
        continued_finished().file("output_rustyhgm/prev/best_harness/lib.rs"),
        "progenitor\nchild\n"
    );
}

#[test]
fn a_continued_run_records_the_progenitor() {
    assert!(
        continued_finished()
            .root()
            .join("output_rustyhgm/prev/progenitor/metadata.json")
            .is_file()
    );
}

#[test]
fn a_continued_run_logs_its_configuration() {
    assert!(
        continued_finished()
            .file("output_rustyhgm/prev/rustyhgm_outer.log")
            .contains(&format!("Starting HGM run {RUN_ID} with configuration: {{"))
    );
}

#[test]
fn a_continued_run_logs_its_budget() {
    assert!(
        continued_finished()
            .file("output_rustyhgm/prev/rustyhgm_outer.log")
            .contains("\"max_task_evals\":2")
    );
}

fn crowded() -> Ran {
    in_root(
        |root| {
            std::fs::create_dir_all(root.join("out")).unwrap();
            std::fs::write(root.join("out/left_over"), "").unwrap();
        },
        |c| {
            c.args(flags(&[]));
        },
    )
}

#[test]
fn a_fresh_run_into_a_folder_holding_files_fails() {
    assert_eq!(crowded().code, Some(1));
}

#[test]
fn a_fresh_run_into_a_folder_holding_files_says_so() {
    assert!(
        crowded()
            .stderr
            .contains("a fresh run needs an empty --output_dir")
    );
}

#[test]
fn a_fresh_run_into_a_folder_holding_files_leaves_it_alone() {
    assert!(!crowded().root().join("out/progenitor").exists());
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
fn held_out() -> Ran {
    in_root(
        |root| {
            finished_run(root);
            let holdout = root.join("output_rustyhgm/prev/holdout/best_harness");
            std::fs::create_dir_all(&holdout).unwrap();
            std::fs::write(holdout.join("rustyhgm-coding-harness"), "").unwrap();
        },
        |c| {
            let python = swe_cache(&PathBuf::from(c.get_current_dir().unwrap()));
            let python = python.to_string_lossy();
            let mut changes = FINISHED.to_vec();
            changes.push(("--swebench_python", &python));
            c.args(flags(&changes))
                .args(["--holdout_split", "Lite", "--holdout_tasks", "2"]);
        },
    )
}

#[cfg(unix)]
fn holdout_metadata() -> serde_json::Value {
    serde_json::from_str(
        &held_out().file("output_rustyhgm/prev/holdout/best_harness/metadata.json"),
    )
    .unwrap()
}

#[cfg(unix)]
#[test]
fn a_holdout_split_runs_after_the_search() {
    let ran = held_out();
    assert_eq!(ran.code, Some(0), "{}{}", ran.stdout, ran.stderr);
}

#[cfg(unix)]
#[test]
fn a_holdout_split_scores_the_best_harness_after_the_run() {
    assert!(held_out().stdout.contains("Held-out Lite: solved 0/2\n"));
}

#[cfg(unix)]
#[test]
fn a_holdout_score_is_recorded_for_the_best_harness() {
    assert_eq!(holdout_metadata()["run_id"], "best_harness");
}

#[cfg(unix)]
#[test]
fn a_holdout_score_records_the_dataset_names() {
    assert!(holdout_metadata()["swe_dnames"].is_array());
}

fn holdout_without_dataset() -> Ran {
    in_root(finished_run, |c| {
        c.args(flags(&FINISHED))
            .args(["--holdout_split", "Lite", "--holdout_tasks", "1"]);
    })
}

#[test]
fn a_holdout_split_needs_its_dataset() {
    assert_eq!(holdout_without_dataset().code, Some(1));
}

#[test]
fn a_missing_holdout_dataset_is_reported() {
    let ran = holdout_without_dataset();
    assert!(ran.stderr.starts_with("rustyhgm: "), "{}", ran.stderr);
}

#[test]
fn a_missing_holdout_dataset_still_writes_the_best_harness() {
    assert!(
        holdout_without_dataset()
            .root()
            .join("output_rustyhgm/prev/best_harness/lib.rs")
            .is_file()
    );
}

fn continued(root: &std::path::Path, nodes: &[&str], n_task_evals: usize) {
    let prev = root.join("output_rustyhgm/prev");
    metadata(&prev, "progenitor", serde_json::json!({}));
    let mut records = Vec::new();
    for (i, commit) in nodes.iter().enumerate() {
        metadata(
            &prev,
            commit,
            serde_json::json!({"parent_commit": "progenitor"}),
        );
        records.push(serde_json::json!({"commit_id": commit, "id": i + 1, "parent_id": 0}));
    }
    std::fs::write(prev.join("init_evaluated_tasks.json"), "[]").unwrap();
    std::fs::write(
        prev.join("rustyhgm_metadata.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({"run_id": RUN_ID, "n_task_evals": n_task_evals, "nodes": records})
        ),
    )
    .unwrap();
}

fn overspent() -> Ran {
    in_root(
        |root| continued(root, &["c1", "c2", "c3"], 1),
        |c| {
            c.args(flags(&[
                ("--continue_from", "output_rustyhgm/prev"),
                ("--output_dir", "output_rustyhgm/prev"),
                ("--max_task_evals", "7"),
            ]));
        },
    )
}

#[test]
fn a_continued_run_evaluates_until_the_budget_is_spent() {
    assert_eq!(overspent().code, Some(1));
}

#[test]
fn a_continued_run_logs_the_evaluation_error() {
    assert!(
        overspent()
            .file("output_rustyhgm/prev/rustyhgm_outer.log")
            .contains("Error: ")
    );
}

fn full_eval() -> Ran {
    in_root(
        |_| {},
        |c| {
            c.args(flags(&[("--full_eval", ""), ("--output_dir", "full")]));
        },
    )
}

#[test]
fn a_full_evaluation_without_its_dataset_fails() {
    assert_eq!(full_eval().code, Some(1));
}

#[test]
fn a_full_evaluation_without_its_dataset_says_so() {
    assert!(
        full_eval()
            .stderr
            .contains("rustyhgm: downloading the dataset")
    );
}

fn missing_records() -> Ran {
    in_root(
        |_| {},
        |c| {
            c.args(flags(&[("--continue_from", "output_rustyhgm/none")]));
        },
    )
}

fn keyless() -> Ran {
    in_root(
        |_| {},
        |c| {
            c.args(flags(&[("--api_key_file", "gone")]));
        },
    )
}

#[test]
fn a_run_without_its_key_file_fails() {
    assert_eq!(keyless().code, Some(1));
}

#[test]
fn a_run_without_its_key_file_names_it() {
    let ran = keyless();
    let named = format!("{}gone: ", std::path::MAIN_SEPARATOR);
    assert!(
        ran.stderr
            .strip_prefix("rustyhgm: ")
            .is_some_and(|rest| rest.contains(&named)),
        "{}",
        ran.stderr
    );
}

const FRESH_PROGENITOR: &str = "out/progenitor/src";

fn fresh() -> Ran {
    in_root(
        |_| {},
        |c| {
            c.args(flags(&[]));
        },
    )
}

#[test]
fn a_fresh_run_writes_the_built_in_progenitor() {
    assert!(
        fresh()
            .root()
            .join(FRESH_PROGENITOR)
            .join("src/coding-harness/main.rs")
            .is_file()
    );
}

#[test]
fn a_fresh_run_gives_the_progenitor_its_dockerfile() {
    assert!(
        fresh()
            .root()
            .join(FRESH_PROGENITOR)
            .join("Dockerfile")
            .is_file()
    );
}

#[test]
fn a_fresh_run_leaves_the_controller_out_of_the_progenitor() {
    assert!(
        !fresh()
            .root()
            .join(FRESH_PROGENITOR)
            .join("src/rustyhgm")
            .exists()
    );
}

#[test]
fn continuing_a_missing_run_fails() {
    assert_eq!(missing_records().code, Some(1));
}

#[test]
fn continuing_a_missing_run_names_the_missing_records() {
    assert!(missing_records().stderr.contains("rustyhgm_metadata.jsonl"));
}

fn homeless() -> Ran {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg("cd \"$1\" && rmdir \"$1\" && shift && exec \"$@\"")
        .arg("sh")
        .arg(&gone)
        .arg(binary("rustyhgm"))
        .args(flags(&[]))
        .output()
        .unwrap();
    ran(tmp, out)
}

#[test]
fn rustyhgm_needs_a_working_directory() {
    assert_eq!(homeless().code, Some(1));
}

#[test]
fn a_missing_working_directory_is_reported() {
    assert!(homeless().stderr.starts_with("rustyhgm: "));
}
