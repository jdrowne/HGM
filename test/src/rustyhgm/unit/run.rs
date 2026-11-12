use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustyhgm::cli::Cli;
use rustyhgm::node::Evaluator;
use rustyhgm::run::{
    Holdout, Init, bench_for, check_fresh_output_dir, finish, holdout, image_name, improve_dataset,
    initialize_run, new_run_id, remove_images, run, run_id_of, run_tasks, settings,
};
use rustyhgm::search::Settings;
use rustyhgm_bench::dataset::Hub;
use rustyhgm_bench::tasks::{polyglot_run_tasks, swe_run_tasks};
use rustyhgm_core::config::{ExpansionStatistic, HgmConfig};
use rustyhgm_core::log::Log;
use rustyhgm_sandbox::Docker;
use serde_json::json;

use crate::common::{FakeBench, FakeBuild, cli, node, performance, read_json, service};

const RUN_ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

#[test]
fn an_image_is_named_after_the_run() {
    assert_eq!(
        image_name("rustyhgm-harness", RUN_ID),
        format!("rustyhgm-harness-{RUN_ID}:latest")
    );
}

#[test]
fn each_new_run_gets_its_own_id() {
    assert_ne!(new_run_id(), new_run_id());
}

#[test]
fn a_new_run_id_is_a_uuid() {
    assert_eq!(new_run_id().len(), RUN_ID.len());
}

fn record_with(run_id: serde_json::Value) -> Result<String, String> {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("rustyhgm_metadata.jsonl"),
        format!(
            "{}\n",
            json!({"run_id": run_id, "n_task_evals": 0, "nodes": []})
        ),
    )
    .unwrap();
    run_id_of(tmp.path())
}

#[test]
fn a_continued_run_reads_its_id_back() {
    assert_eq!(record_with(json!(RUN_ID)).unwrap(), RUN_ID);
}

#[test]
fn a_continued_run_keeps_its_id_in_lower_case() {
    assert_eq!(record_with(json!(RUN_ID.to_uppercase())).unwrap(), RUN_ID);
}

#[test]
fn a_record_without_a_run_id_cannot_be_continued() {
    assert!(record_with(json!(null)).unwrap_err().contains("run_id"));
}

#[test]
fn a_run_id_that_is_not_a_uuid_cannot_be_continued() {
    assert!(record_with(json!("x")).unwrap_err().contains("run_id x"));
}

#[test]
fn a_missing_output_directory_is_fresh() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(check_fresh_output_dir(&tmp.path().join("out")), Ok(()));
}

#[test]
fn an_empty_output_directory_is_fresh() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(check_fresh_output_dir(tmp.path()), Ok(()));
}

#[test]
fn an_output_directory_holding_files_is_not_fresh() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("x"), "").unwrap();
    assert!(
        check_fresh_output_dir(tmp.path())
            .unwrap_err()
            .contains("--continue_from")
    );
}

#[cfg(unix)]
fn fake_docker(script: &str) -> (tempfile::TempDir, Docker) {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let program = tmp.path().join("docker");
    fs::write(
        &program,
        format!("#!/bin/sh\necho \"$@\" >> \"$0.calls\"\n{script}\n"),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let docker = Docker::new(program.to_string_lossy());
    (tmp, docker)
}

#[cfg(unix)]
fn removal_calls(script: &str) -> Vec<String> {
    let (tmp, docker) = fake_docker(script);
    remove_images(&docker, &["i:latest".to_owned()], &Log::discard());
    fs::read_to_string(tmp.path().join("docker.calls"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
#[test]
fn a_finished_run_removes_its_images() {
    assert_eq!(
        removal_calls("exit 0"),
        [
            "image inspect --format {{.Id}} i:latest",
            "image rm -f i:latest"
        ]
    );
}

#[cfg(unix)]
#[test]
fn an_image_that_is_gone_is_not_removed() {
    assert_eq!(removal_calls("exit 1").len(), 1);
}

#[cfg(unix)]
#[test]
fn a_failed_removal_does_not_stop_the_next_one() {
    let (tmp, docker) = fake_docker("[ \"$2\" = rm ] && exit 1\nexit 0");
    remove_images(&docker, &["a".to_owned(), "b".to_owned()], &Log::discard());
    assert_eq!(
        fs::read_to_string(tmp.path().join("docker.calls"))
            .unwrap()
            .lines()
            .count(),
        4
    );
}

#[test]
fn the_search_settings_come_from_the_flags() {
    let config = cli(&[
        ("--max_task_evals", "7"),
        ("--max_workers", "3"),
        ("--alpha", "0.5"),
        ("--beta", "2"),
        ("--cool_down", ""),
        ("--n_pseudo_descendant_evals", "13"),
        ("--expansion_statistic", "own"),
        ("--eval_random_level", "0.25"),
    ])
    .config();
    assert_eq!(
        settings(&config, Duration::from_millis(3)),
        Settings {
            max_task_evals: 7,
            max_workers: 3,
            alpha: 0.5,
            beta: 2.0,
            cool_down: true,
            n_pseudo_descendant_evals: 13,
            expansion_statistic: ExpansionStatistic::Own,
            eval_random_level: 0.25,
            jitter: Duration::from_millis(3),
        }
    );
}

fn offline(root: &Path) -> Hub {
    Hub {
        endpoint: "http://127.0.0.1:9".into(),
        cache_dir: root.join("cache"),
    }
}

fn full_eval() -> HgmConfig {
    cli(&[("--full_eval", "")]).config()
}

fn verified_cache(root: &Path) {
    let cache = root.join("cache/princeton-nlp/SWE-bench_Verified/test");
    fs::create_dir_all(&cache).unwrap();
    let fixture = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures/rows.parquet");
    fs::copy(fixture, cache.join("rows.parquet")).unwrap();
    fs::write(cache.join("files.json"), r#"["rows.parquet"]"#).unwrap();
}

#[test]
fn swe_runs_use_the_swe_task_order() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        run_tasks(&cli(&[]).config(), &offline(tmp.path())).unwrap(),
        swe_run_tasks()
    );
}

#[test]
fn a_full_evaluation_needs_the_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(run_tasks(&full_eval(), &offline(tmp.path())).is_err());
}

#[test]
fn the_swe_improve_dataset_needs_the_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(improve_dataset(false, tmp.path(), &offline(tmp.path())).is_err());
}

#[test]
fn a_full_evaluation_runs_every_verified_task() {
    let tmp = tempfile::tempdir().unwrap();
    verified_cache(tmp.path());
    let mut full = run_tasks(&full_eval(), &offline(tmp.path())).unwrap();
    full.sort();
    assert_eq!(
        full,
        [
            "astropy__astropy-12907",
            "django__django-11099",
            "sympy__sympy-20590"
        ]
    );
}

#[test]
fn the_swe_improve_dataset_is_the_verified_set() {
    let tmp = tempfile::tempdir().unwrap();
    verified_cache(tmp.path());
    assert_eq!(
        improve_dataset(false, tmp.path(), &offline(tmp.path()))
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn polyglot_runs_use_the_polyglot_task_order() {
    let tmp = tempfile::tempdir().unwrap();
    let mut config = full_eval();
    config.evaluation.polyglot = true;
    assert_eq!(
        run_tasks(&config, &offline(tmp.path())).unwrap(),
        polyglot_run_tasks()
    );
}

fn polyglot_dataset(contents: Option<&str>) -> Result<Vec<serde_json::Value>, String> {
    let tmp = tempfile::tempdir().unwrap();
    if let Some(text) = contents {
        let path = tmp.path().join("polyglot/polyglot_benchmark_metadata.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    improve_dataset(true, &tmp.path().join("polyglot"), &offline(tmp.path()))
}

#[test]
fn the_polyglot_improve_dataset_needs_its_file() {
    assert!(polyglot_dataset(None).is_err());
}

#[test]
fn the_polyglot_improve_dataset_is_read_from_its_file() {
    assert_eq!(
        polyglot_dataset(Some(r#"[{"instance_id": "x"}]"#)).unwrap(),
        [json!({"instance_id": "x"})]
    );
}

#[test]
fn a_junk_polyglot_dataset_is_an_error() {
    assert!(polyglot_dataset(Some("[")).is_err());
}

fn bench_error(polyglot: bool) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let mut config = cli(&[]).config();
    config.evaluation.polyglot = polyglot;
    let out = tmp.path().join("out");
    fs::create_dir_all(out.join("c")).unwrap();
    let bench = bench_for(&config, &service(), tmp.path(), &cli(&[]).tools(tmp.path()));
    Evaluator {
        bench: bench.as_ref(),
        output_dir: &out,
        max_workers: 1,
        builder: &FakeBuild::default(),
    }
    .eval_harness("c", &["t".to_owned()])
    .unwrap_err()
}

#[test]
fn the_swe_bench_is_used_by_default() {
    assert!(!bench_error(false).starts_with("Dataset file not found: "));
}

#[test]
fn the_polyglot_bench_follows_the_benchmark_flag() {
    let e = bench_error(true);
    assert!(e.starts_with("Dataset file not found: "), "{e}");
}

fn init<'a>(out: &'a Path, tasks: &'a [String], prev: Option<&'a Path>) -> Init<'a> {
    Init {
        output_dir: out,
        image_name: "rustyhgm-harness-r:latest",
        continue_from: prev,
        total_tasks: tasks,
        max_workers: 3,
    }
}

fn two_tasks() -> Vec<String> {
    vec!["a".to_owned(), "b".to_owned()]
}

#[cfg(unix)]
struct Started {
    tmp: tempfile::TempDir,
    bench: FakeBench,
    start: rustyhgm::run::Start,
}

#[cfg(unix)]
impl Started {
    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn seen(&self) -> Vec<crate::common::Seen> {
        self.bench.seen.lock().unwrap().clone()
    }
}

#[cfg(unix)]
fn started() -> Started {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bench = FakeBench::resolving(&["a"]);
    let start = initialize_run(
        &init(&root.join("out"), &two_tasks(), None),
        &bench,
        &Docker::new("true"),
        &Log::discard(),
    )
    .unwrap();
    Started { tmp, bench, start }
}

#[cfg(unix)]
#[test]
fn a_new_run_points_at_the_progenitor_inside_the_run() {
    let s = started();
    assert_eq!(s.start.src_path, s.root().join("out/progenitor/src"));
}

#[cfg(unix)]
#[test]
fn a_new_run_copies_the_dockerfile() {
    assert!(
        started()
            .root()
            .join("out/progenitor/src/Dockerfile")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn a_new_run_writes_the_progenitor_into_the_run() {
    assert!(
        started()
            .root()
            .join("out/progenitor/src/src/coding-harness/main.rs")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn a_new_run_evaluates_the_progenitor_once() {
    assert_eq!(started().seen().len(), 1);
}

#[cfg(unix)]
#[test]
fn the_progenitor_evaluation_is_filed_under_progenitor() {
    assert_eq!(started().seen()[0].commit, "progenitor");
}

#[cfg(unix)]
#[test]
fn the_progenitor_evaluation_covers_every_task() {
    assert_eq!(started().seen()[0].tasks, two_tasks());
}

#[cfg(unix)]
#[test]
fn the_progenitor_evaluation_caps_its_workers() {
    assert_eq!(started().seen()[0].max_workers, 2);
}

#[cfg(unix)]
#[test]
fn a_new_run_starts_with_the_root_alone() {
    assert_eq!(started().start.tree.len(), 1);
}

#[cfg(unix)]
#[test]
fn the_root_holds_the_progenitor_measures() {
    assert_eq!(
        started().start.tree.get(0).unwrap().utility_measures,
        [1.0, 0.0]
    );
}

#[cfg(unix)]
#[test]
fn the_progenitor_tasks_count_as_submitted() {
    assert_eq!(started().start.submitted_ids[&0].len(), 2);
}

#[cfg(unix)]
#[test]
fn a_new_run_has_spent_no_budget() {
    assert_eq!(started().start.n_task_evals, 0);
}

#[cfg(unix)]
#[test]
fn a_new_run_has_no_initially_evaluated_tasks() {
    assert!(started().start.init_evaluated_tasks.is_empty());
}

#[cfg(unix)]
#[test]
fn the_progenitor_node_is_named_progenitor() {
    assert_eq!(
        read_json(&started().root().join("out/progenitor/metadata.json"))["run_id"],
        "progenitor"
    );
}

#[test]
fn a_missing_docker_fails_the_setup() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    assert!(
        initialize_run(
            &init(&root.join("out"), &["a".to_owned()], None),
            &FakeBench::resolving(&[]),
            &Docker::new("rustyhgm-no-such-docker-program"),
            &Log::discard()
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn a_failing_progenitor_evaluation_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let failing = FakeBench {
        fail: true,
        ..FakeBench::resolving(&[])
    };
    assert_eq!(
        initialize_run(
            &init(&root.join("out"), &["a".to_owned()], None),
            &failing,
            &Docker::new("true"),
            &Log::discard()
        )
        .err(),
        Some("bench failed".to_owned())
    );
}

fn previous(prev: &Path) {
    fs::write(prev.join("init_evaluated_tasks.json"), "[\"a\"]").unwrap();
    let record = |n: usize, nodes: serde_json::Value| {
        serde_json::to_string_pretty(&json!({"run_id": RUN_ID, "n_task_evals": n, "nodes": nodes}))
            .unwrap()
    };
    fs::write(
        prev.join("rustyhgm_metadata.jsonl"),
        format!(
            "{}\n{}\n",
            record(0, json!([])),
            record(3, json!([
                {"commit_id": "c1", "id": 1, "parent_id": 0, "mean_utility": 1.0, "num_evals": 1},
                {"commit_id": "c2", "id": 2, "parent_id": 1, "mean_utility": 0.5, "num_evals": 2}
            ]))
        ),
    )
    .unwrap();
}

fn continued() -> (FakeBench, rustyhgm::run::Start) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("run");
    fs::create_dir_all(&out).unwrap();
    previous(&out);
    node(&out, "progenitor", None, performance(&["a"], &["b"], &[]));
    node(
        &out,
        "c1",
        Some("progenitor"),
        performance(&["a"], &[], &[]),
    );
    node(&out, "c2", Some("c1"), performance(&["a"], &["b"], &[]));
    let bench = FakeBench::resolving(&[]);
    let start = initialize_run(
        &init(&out, &two_tasks(), Some(&out)),
        &bench,
        &Docker::new("rustyhgm-no-such-docker-program"),
        &Log::discard(),
    )
    .unwrap();
    (bench, start)
}

#[test]
fn a_continued_run_evaluates_nothing() {
    assert!(continued().0.seen.lock().unwrap().is_empty());
}

#[test]
fn a_continued_run_reads_the_progenitor_measures_from_the_run() {
    assert_eq!(
        continued().1.tree.get(0).unwrap().utility_measures,
        [1.0, 0.0]
    );
}

#[test]
fn a_continued_run_uses_the_progenitor_source_inside_the_run() {
    assert!(continued().1.src_path.ends_with("run/progenitor/src"));
}

#[test]
fn a_continued_run_rebuilds_the_tree_from_the_last_record() {
    assert_eq!(continued().1.tree.len(), 3);
}

#[test]
fn a_continued_run_keeps_the_parents() {
    assert_eq!(continued().1.tree.get(2).unwrap().parent_id, Some(1));
}

#[test]
fn a_continued_run_links_the_children() {
    assert_eq!(continued().1.tree.get(0).unwrap().children, [1]);
}

#[test]
fn a_continued_run_reads_the_node_measures() {
    assert_eq!(
        continued().1.tree.get(2).unwrap().utility_measures,
        [1.0, 0.0]
    );
}

#[test]
fn a_continued_run_keeps_the_spent_budget() {
    assert_eq!(continued().1.n_task_evals, 3);
}

#[test]
fn a_continued_run_keeps_the_initially_evaluated_tasks() {
    assert_eq!(continued().1.init_evaluated_tasks, ["a"]);
}

#[test]
fn a_continued_run_reads_the_submitted_tasks() {
    assert_eq!(continued().1.submitted_ids[&1].len(), 1);
}

fn continue_error(prepare: impl FnOnce(&Path)) -> Option<String> {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("run");
    fs::create_dir_all(&out).unwrap();
    prepare(&out);
    initialize_run(
        &init(&out, &["a".to_owned()], Some(&out)),
        &FakeBench::resolving(&[]),
        &Docker::new("rustyhgm-no-such-docker-program"),
        &Log::discard(),
    )
    .err()
}

#[test]
fn a_continued_run_needs_its_initially_evaluated_tasks() {
    assert!(
        continue_error(|_| {})
            .unwrap()
            .contains("init_evaluated_tasks.json")
    );
}

#[test]
fn a_continued_run_needs_its_records() {
    assert!(
        continue_error(|out| fs::write(out.join("init_evaluated_tasks.json"), "[]").unwrap())
            .unwrap()
            .contains("rustyhgm_metadata.jsonl")
    );
}

#[test]
fn a_continued_run_needs_complete_records() {
    assert!(
        continue_error(|out| {
            fs::write(out.join("init_evaluated_tasks.json"), "[]").unwrap();
            fs::write(
                out.join("rustyhgm_metadata.jsonl"),
                "{\"nodes\": [{\"id\": 1}]}",
            )
            .unwrap();
        })
        .is_some()
    );
}

#[test]
fn a_continued_run_needs_each_node_folder() {
    assert!(
        continue_error(|out| {
            previous(out);
            node(out, "progenitor", None, performance(&[], &[], &[]));
        })
        .unwrap()
        .contains("c1")
    );
}

#[test]
fn a_continued_run_needs_its_progenitor() {
    assert!(
        continue_error(previous)
            .unwrap()
            .contains("run/progenitor: ")
    );
}

fn run_args(extra: impl FnOnce(&mut Cli)) -> Cli {
    let mut args = cli(&[]);
    extra(&mut args);
    args
}

fn run_error(prepare: impl FnOnce(&Path), extra: impl FnOnce(&mut Cli)) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("key"), "k\n").unwrap();
    prepare(root);
    run(&run_args(extra), root, Duration::ZERO).unwrap_err()
}

#[test]
fn a_run_without_its_key_file_stops_before_any_work() {
    assert!(
        run_error(
            |root| fs::remove_file(root.join("key")).unwrap(),
            |a| a.polyglot = true
        )
        .contains("key: ")
    );
}

#[test]
fn a_run_without_its_key_file_fetches_no_polyglot_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    run(&run_args(|a| a.polyglot = true), root, Duration::ZERO).unwrap_err();
    assert!(!root.join("polyglot").exists());
}

#[test]
fn a_fresh_run_into_a_folder_holding_files_stops_early() {
    assert!(
        run_error(
            |root| {
                fs::create_dir_all(root.join("out")).unwrap();
                fs::write(root.join("out/x"), "").unwrap();
            },
            |_| {}
        )
        .contains("a fresh run needs an empty --output_dir")
    );
}

#[test]
fn a_refused_fresh_run_leaves_the_folder_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("out")).unwrap();
    fs::write(root.join("out/x"), "").unwrap();
    fs::write(root.join("key"), "k\n").unwrap();
    run(&run_args(|_| {}), root, Duration::ZERO).unwrap_err();
    assert_eq!(fs::read_dir(root.join("out")).unwrap().count(), 1);
}

#[test]
fn a_continued_run_without_its_record_says_so() {
    assert!(
        run_error(
            |root| fs::create_dir_all(root.join("out")).unwrap(),
            |a| a.continue_from = Some("out".into())
        )
        .contains("rustyhgm_metadata.jsonl")
    );
}

#[test]
fn a_run_with_an_output_directory_under_a_file_stops_early() {
    assert!(
        !run_error(
            |root| fs::write(root.join("file"), "").unwrap(),
            |a| a.output_dir = "file/out".into()
        )
        .is_empty()
    );
}

#[test]
fn a_run_that_cannot_open_its_log_says_so() {
    assert!(
        run_error(
            |root| fs::create_dir_all(root.join("out/rustyhgm_outer.log")).unwrap(),
            |a| a.continue_from = Some("out".into())
        )
        .contains("rustyhgm_outer.log")
    );
}

#[test]
fn a_run_with_a_bad_epsilon_stops_before_any_work() {
    assert!(run_error(|_| {}, |a| a.epsilon = 1.0).starts_with("epsilon must be between 0 and 1"));
}

fn polyglot_holdout(a: &mut Cli) {
    a.polyglot = true;
    a.holdout_split = Some("Lite".into());
    a.holdout_tasks = Some(1);
}

#[test]
fn a_polyglot_run_with_a_holdout_stops_before_any_work() {
    assert!(run_error(|_| {}, polyglot_holdout).contains("--holdout_split"));
}

fn refused_root() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    run(&run_args(|a| a.epsilon = 1.0), root, Duration::ZERO).unwrap_err();
    run(&run_args(polyglot_holdout), root, Duration::ZERO).unwrap_err();
    tmp
}

#[test]
fn a_refused_run_makes_no_output_folder() {
    assert!(!refused_root().path().join("out").exists());
}

#[test]
fn a_refused_run_fetches_no_polyglot_dataset() {
    assert!(!refused_root().path().join("polyglot").exists());
}

fn finished_run() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("run");
    fs::create_dir_all(out.join("progenitor/src")).unwrap();
    fs::write(out.join("progenitor/src/lib.rs"), "progenitor\n").unwrap();
    fs::create_dir_all(out.join("c1")).unwrap();
    fs::write(
        out.join("c1/metadata.json"),
        json!({"parent_commit": "progenitor"}).to_string(),
    )
    .unwrap();
    fs::write(
        out.join("c1/model_patch.diff"),
        "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n progenitor\n+child\n",
    )
    .unwrap();
    let record = json!({"run_id": RUN_ID, "n_task_evals": 2, "nodes": [{"commit_id": "c1", "id": 1, "parent_id": 0, "mean_utility": 0.5, "num_evals": 2}]});
    fs::write(out.join("rustyhgm_metadata.jsonl"), format!("{record}\n")).unwrap();
    (tmp, out)
}

#[test]
fn finish_writes_the_best_harness_source_into_the_run() {
    let (_tmp, out) = finished_run();
    finish(
        &cli(&[]),
        &service(),
        &cli(&[]).tools(&out),
        &out,
        "rustyhgm-holdout-harness-r:latest",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(out.join("best_harness/lib.rs")).unwrap(),
        "progenitor\nchild\n"
    );
}

#[test]
fn finish_needs_the_run_records() {
    let (_tmp, out) = finished_run();
    fs::remove_file(out.join("rustyhgm_metadata.jsonl")).unwrap();
    assert!(
        finish(
            &cli(&[]),
            &service(),
            &cli(&[]).tools(&out),
            &out,
            "rustyhgm-holdout-harness-r:latest",
        )
        .unwrap_err()
        .contains("rustyhgm_metadata.jsonl")
    );
}

#[test]
fn finish_needs_the_progenitor_source() {
    let (_tmp, out) = finished_run();
    fs::remove_dir_all(out.join("progenitor")).unwrap();
    assert!(
        finish(
            &cli(&[]),
            &service(),
            &cli(&[]).tools(&out),
            &out,
            "rustyhgm-holdout-harness-r:latest",
        )
        .unwrap_err()
        .contains("the progenitor's source is missing")
    );
}

fn held_out(results: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let cache = root.join("cache/princeton-nlp/SWE-bench_Lite/test");
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/src/bench/unit/fixtures/rows.parquet"),
        cache.join("rows.parquet"),
    )
    .unwrap();
    fs::write(cache.join("files.json"), r#"["rows.parquet"]"#).unwrap();
    fs::write(root.join("file"), "").unwrap();
    let harness_dir = root.join("best_harness");
    fs::create_dir_all(&harness_dir).unwrap();
    holdout(&Holdout {
        harness_dir: &harness_dir,
        results_dir: &root.join(results),
        image_name: "rustyhgm-holdout-harness-r:latest",
        split: "Lite",
        n_tasks: 1,
        llm: "m",
        service: &service(),
        tools: &cli(&[]).tools(root),
        timeout: 1,
        max_workers: 1,
    })
    .unwrap_err()
}

#[test]
fn a_holdout_cannot_record_under_a_file() {
    assert!(held_out("file/holdout").contains("file/holdout"));
}

#[test]
fn a_holdout_fails_when_the_harness_cannot_be_built() {
    assert!(held_out("holdout").contains("No such file or directory"));
}
