use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hgm::cli::Cli;
use hgm::node::Evaluator;
use hgm::run::{
    Init, bench_for, finish, improve_dataset, initial_folder, initialize_run, resolve_output_dir,
    run, run_tasks, settings,
};
use hgm::search::Settings;
use hgm_bench::dataset::Hub;
use hgm_bench::tasks::{polyglot_run_tasks, swe_run_tasks};
use hgm_core::config::{ExpansionStatistic, HgmConfig};
use hgm_core::log::Log;
use hgm_sandbox::Docker;
use serde_json::json;

use crate::common::{FakeBench, FakeBuild, cli, node, performance, read_json};

fn resolved(output_dir: &str) -> (PathBuf, String) {
    resolve_output_dir(output_dir, Path::new("/w"))
}

#[test]
fn an_output_directory_names_the_run() {
    assert_eq!(
        resolved("runs/x/"),
        (PathBuf::from("/w/runs/x/"), "x".to_owned())
    );
}

#[test]
fn the_root_as_output_directory_has_no_name() {
    assert_eq!(resolved("/"), (PathBuf::from("/"), String::new()));
}

#[test]
fn polyglot_runs_start_from_the_polyglot_folder() {
    assert_eq!(initial_folder(true), "initial_polyglot");
}

#[test]
fn swe_runs_start_from_the_swe_folder() {
    assert_eq!(initial_folder(false), "initial_swe");
}

#[test]
fn the_search_settings_come_from_the_flags() {
    let config = cli(&[
        ("--max_task_evals", "7"),
        ("--max_workers", "3"),
        ("--alpha", "0.5"),
        ("--beta", "2"),
        ("--cool_down", "true"),
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
    cli(&[("--full_eval", "true")]).config()
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
    improve_dataset(true, tmp.path(), &offline(tmp.path()))
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
    let docker = Docker::new("hgm-no-such-docker-program");
    let bench = bench_for(&config, tmp.path(), &docker, &offline(tmp.path()));
    Evaluator {
        bench: bench.as_ref(),
        output_dir: &out,
        max_workers: 1,
        builder: &FakeBuild::default(),
    }
    .eval_agent("c", &["t".to_owned()])
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

#[cfg(unix)]
fn agent_source(root: &Path) {
    fs::write(
        root.join(".dockerignore"),
        "initial_swe\ninitial_polyglot\nout\n",
    )
    .unwrap();
    fs::write(root.join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(root.join("coding_agent.py"), "").unwrap();
}

fn init<'a>(
    root: &'a Path,
    out: &'a Path,
    tasks: &'a [String],
    prev: Option<&'a Path>,
) -> Init<'a> {
    Init {
        root_dir: root,
        output_dir: out,
        agent_name: "agent",
        polyglot: false,
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
    start: hgm::run::Start,
}

#[cfg(unix)]
impl Started {
    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn seen(&self) -> Vec<crate::common::Seen> {
        self.bench.seen.lock().unwrap().clone()
    }

    fn again(&self) -> hgm::run::Start {
        initialize_run(
            &init(self.root(), &self.root().join("out"), &two_tasks(), None),
            &self.bench,
            &Docker::new("true"),
            &Log::discard(),
        )
        .unwrap()
    }
}

#[cfg(unix)]
fn started() -> Started {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    agent_source(root);
    let bench = FakeBench::resolving(&["a"]);
    let start = initialize_run(
        &init(root, &root.join("out"), &two_tasks(), None),
        &bench,
        &Docker::new("true"),
        &Log::discard(),
    )
    .unwrap();
    Started { tmp, bench, start }
}

#[cfg(unix)]
#[test]
fn a_new_run_points_at_the_prepared_source() {
    let s = started();
    assert_eq!(s.start.src_path, s.root().join("initial_swe/agent/src"));
}

#[cfg(unix)]
#[test]
fn a_new_run_copies_the_agent_source() {
    assert!(
        started()
            .root()
            .join("initial_swe/agent/src/coding_agent.py")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn a_new_run_copies_the_dockerfile() {
    assert!(
        started()
            .root()
            .join("initial_swe/agent/src/Dockerfile")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn a_new_run_seeds_the_initial_node() {
    assert!(
        started()
            .root()
            .join("out/initial/src/coding_agent.py")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn a_new_run_evaluates_the_initial_agent_once() {
    assert_eq!(started().seen().len(), 1);
}

#[cfg(unix)]
#[test]
fn the_initial_evaluation_names_the_agent() {
    assert_eq!(started().seen()[0].commit, "agent");
}

#[cfg(unix)]
#[test]
fn the_initial_evaluation_covers_every_task() {
    assert_eq!(started().seen()[0].tasks, two_tasks());
}

#[cfg(unix)]
#[test]
fn the_initial_evaluation_caps_its_workers() {
    assert_eq!(started().seen()[0].max_workers, 2);
}

#[cfg(unix)]
#[test]
fn a_new_run_starts_with_the_root_alone() {
    assert_eq!(started().start.tree.len(), 1);
}

#[cfg(unix)]
#[test]
fn the_root_holds_the_initial_measures() {
    assert_eq!(
        started().start.tree.get(0).unwrap().utility_measures,
        [1.0, 0.0]
    );
}

#[cfg(unix)]
#[test]
fn the_initial_tasks_count_as_submitted() {
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
fn a_second_start_reads_the_initial_measures_back() {
    assert_eq!(
        started().again().tree.get(0).unwrap().utility_measures,
        [1.0, 0.0]
    );
}

#[cfg(unix)]
#[test]
fn a_second_start_does_not_evaluate_again() {
    let s = started();
    s.again();
    assert_eq!(s.seen().len(), 1);
}

#[cfg(unix)]
#[test]
fn the_initial_node_is_named_initial() {
    assert_eq!(
        read_json(&started().root().join("out/initial/metadata.json"))["run_id"],
        "initial"
    );
}

#[test]
fn a_missing_docker_fails_the_setup() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    assert!(
        initialize_run(
            &init(root, &root.join("out"), &["a".to_owned()], None),
            &FakeBench::resolving(&[]),
            &Docker::new("hgm-no-such-docker-program"),
            &Log::discard()
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn a_failing_initial_evaluation_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    agent_source(root);
    let failing = FakeBench {
        fail: true,
        ..FakeBench::resolving(&[])
    };
    assert_eq!(
        initialize_run(
            &init(root, &root.join("out"), &["a".to_owned()], None),
            &failing,
            &Docker::new("true"),
            &Log::discard()
        )
        .err(),
        Some("bench failed".to_owned())
    );
}

#[test]
fn an_initial_folder_without_source_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("initial_swe/agent")).unwrap();
    let e = initialize_run(
        &init(root, &root.join("out"), &["a".to_owned()], None),
        &FakeBench::resolving(&[]),
        &Docker::new("true"),
        &Log::discard(),
    )
    .err();
    assert!(e.unwrap().contains("initial"));
}

fn previous(prev: &Path) {
    fs::write(prev.join("init_evaluated_tasks.json"), "[\"a\"]").unwrap();
    let record = |n: usize, nodes: serde_json::Value| {
        serde_json::to_string_pretty(&json!({"n_task_evals": n, "nodes": nodes})).unwrap()
    };
    fs::write(
        prev.join("hgm_metadata.jsonl"),
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

fn continued() -> (FakeBench, hgm::run::Start) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("run");
    fs::create_dir_all(&out).unwrap();
    previous(&out);
    node(
        &root.join("initial_swe"),
        "agent",
        None,
        performance(&["a"], &["b"], &[]),
    );
    node(&out, "c1", Some("initial"), performance(&["a"], &[], &[]));
    node(&out, "c2", Some("c1"), performance(&["a"], &["b"], &[]));
    let bench = FakeBench::resolving(&[]);
    let start = initialize_run(
        &init(root, &out, &two_tasks(), Some(&out)),
        &bench,
        &Docker::new("hgm-no-such-docker-program"),
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
        &init(root, &out, &["a".to_owned()], Some(&out)),
        &FakeBench::resolving(&[]),
        &Docker::new("hgm-no-such-docker-program"),
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
            .contains("hgm_metadata.jsonl")
    );
}

#[test]
fn a_continued_run_needs_complete_records() {
    assert!(
        continue_error(|out| {
            fs::write(out.join("init_evaluated_tasks.json"), "[]").unwrap();
            fs::write(out.join("hgm_metadata.jsonl"), "{\"nodes\": [{\"id\": 1}]}").unwrap();
        })
        .is_some()
    );
}

#[test]
fn a_continued_run_needs_each_node_folder() {
    assert!(
        continue_error(|out| {
            previous(out);
            node(out, "initial", None, performance(&[], &[], &[]));
        })
        .unwrap()
        .contains("c1")
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
    prepare(root);
    run(&run_args(extra), root, Duration::ZERO).unwrap_err()
}

#[test]
fn a_run_without_an_agent_name_stops_early() {
    assert!(
        run_error(|_| {}, |a| a.initial_agent_name = String::new())
            .starts_with("--initial_agent_name must not be empty")
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
            |root| fs::create_dir_all(root.join("out/hgm_outer.log")).unwrap(),
            |a| a.output_dir = "out".into()
        )
        .contains("hgm_outer.log")
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
    fs::create_dir_all(out.join("initial/src")).unwrap();
    fs::write(out.join("initial/src/lib.rs"), "seed\n").unwrap();
    fs::create_dir_all(out.join("c1")).unwrap();
    fs::write(
        out.join("c1/metadata.json"),
        json!({"parent_commit": "initial"}).to_string(),
    )
    .unwrap();
    fs::write(
        out.join("c1/model_patch.diff"),
        "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n seed\n+child\n",
    )
    .unwrap();
    let record = json!({"n_task_evals": 2, "nodes": [{"commit_id": "c1", "id": 1, "parent_id": 0, "mean_utility": 0.5, "num_evals": 2}]});
    fs::write(out.join("hgm_metadata.jsonl"), format!("{record}\n")).unwrap();
    (tmp, out)
}

#[test]
fn finish_writes_the_best_agent_source_into_the_run() {
    let (_tmp, out) = finished_run();
    finish(&cli(&[]), &cli(&[]).config(), &out).unwrap();
    assert_eq!(
        fs::read_to_string(out.join("best_agent/lib.rs")).unwrap(),
        "seed\nchild\n"
    );
}

#[test]
fn finish_needs_the_run_records() {
    let (_tmp, out) = finished_run();
    fs::remove_file(out.join("hgm_metadata.jsonl")).unwrap();
    assert!(
        finish(&cli(&[]), &cli(&[]).config(), &out)
            .unwrap_err()
            .contains("hgm_metadata.jsonl")
    );
}
