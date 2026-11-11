use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hgm::cli::RunArgs;
use hgm::node::Evaluator;
use hgm::run::{
    Init, bench_for, improve_dataset, initial_folder, initialize_run, resolve_output_dir, run,
    run_tasks, settings,
};
use hgm_bench::dataset::Hub;
use hgm_bench::tasks::{polyglot_run_tasks, swe_run_tasks};
use hgm_core::config::{HgmConfig, PathConfig};
use hgm_core::log::Log;
use hgm_sandbox::Docker;
use serde_json::json;

use crate::common::{FakeBench, node, performance, read_json, write_json};

fn paths(output_dir: Option<&str>, continue_from: Option<&str>) -> PathConfig {
    PathConfig {
        output_dir: output_dir.map(str::to_owned),
        continue_from: continue_from.map(str::to_owned),
        initial_agent_name: "a".into(),
    }
}

#[test]
fn output_directories_follow_the_python_rules() {
    let cwd = Path::new("/w");
    assert_eq!(
        resolve_output_dir(&paths(Some("runs/x/"), Some("p")), cwd, "now"),
        (PathBuf::from("/w/runs/x/"), "x".to_owned())
    );
    assert_eq!(
        resolve_output_dir(&paths(None, None), cwd, "now"),
        (PathBuf::from("/w/output_hgm/now"), "now".to_owned())
    );
    assert_eq!(
        resolve_output_dir(&paths(None, Some("old/run7")), cwd, "now"),
        (PathBuf::from("/w/output_hgm/run7"), "run7".to_owned())
    );
    assert_eq!(
        resolve_output_dir(&paths(Some("/"), None), cwd, "now"),
        (PathBuf::from("/"), String::new())
    );
    assert_eq!(initial_folder(true), "initial_polyglot");
    assert_eq!(initial_folder(false), "initial_swe");
}

#[test]
fn settings_come_from_the_config() {
    let config = HgmConfig::default();
    let s = settings(&config, Duration::from_millis(3));
    assert_eq!(s.max_task_evals, 800);
    assert_eq!(s.max_workers, 16);
    assert_eq!(s.alpha, 0.6);
    assert_eq!(s.beta, 1.0);
    assert!(!s.cool_down);
    assert_eq!(s.n_pseudo_descendant_evals, 10000);
    assert_eq!(s.eval_random_level, 1.0);
    assert_eq!(s.jitter, Duration::from_millis(3));
}

fn offline(root: &Path) -> Hub {
    Hub {
        endpoint: "http://127.0.0.1:9".into(),
        cache_dir: root.join("cache"),
    }
}

#[test]
fn task_lists_and_datasets_depend_on_the_benchmark() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = offline(tmp.path());
    let mut config = HgmConfig::default();
    assert_eq!(run_tasks(&config, &hub).unwrap(), swe_run_tasks());
    config.evaluation.full_eval = true;
    assert!(run_tasks(&config, &hub).is_err());
    assert!(improve_dataset(false, tmp.path(), &hub).is_err());
    let cache = tmp
        .path()
        .join("cache/princeton-nlp/SWE-bench_Verified/test");
    fs::create_dir_all(&cache).unwrap();
    let fixture = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures/rows.parquet");
    fs::copy(fixture, cache.join("rows.parquet")).unwrap();
    fs::write(cache.join("files.json"), r#"["rows.parquet"]"#).unwrap();
    let mut full = run_tasks(&config, &hub).unwrap();
    full.sort();
    assert_eq!(
        full,
        [
            "astropy__astropy-12907",
            "django__django-11099",
            "sympy__sympy-20590"
        ]
    );
    assert_eq!(improve_dataset(false, tmp.path(), &hub).unwrap().len(), 3);
    config.evaluation.polyglot = true;
    assert_eq!(run_tasks(&config, &hub).unwrap(), polyglot_run_tasks());
    assert!(improve_dataset(true, tmp.path(), &hub).is_err());
    write_json(
        &tmp.path().join("polyglot/polyglot_benchmark_metadata.json"),
        &json!([{"instance_id": "x"}]),
    );
    assert_eq!(
        improve_dataset(true, tmp.path(), &hub).unwrap(),
        [json!({"instance_id": "x"})]
    );
    fs::write(
        tmp.path().join("polyglot/polyglot_benchmark_metadata.json"),
        "[",
    )
    .unwrap();
    assert!(improve_dataset(true, tmp.path(), &hub).is_err());
}

#[test]
fn the_bench_follows_the_benchmark_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = offline(tmp.path());
    let docker = Docker::new("hgm-no-such-docker-program");
    let mut config = HgmConfig::default();
    let out = tmp.path().join("out");
    fs::create_dir_all(out.join("c")).unwrap();
    let swe = bench_for(&config, tmp.path(), &docker, &hub);
    let tasks = ["t".to_owned()];
    assert!(
        Evaluator {
            bench: swe.as_ref(),
            output_dir: &out,
            max_workers: 1,
            init_agent_path: tmp.path()
        }
        .eval_agent("c", &tasks)
        .is_err()
    );
    config.evaluation.polyglot = true;
    let polyglot = bench_for(&config, tmp.path(), &docker, &hub);
    let e = Evaluator {
        bench: polyglot.as_ref(),
        output_dir: &out,
        max_workers: 1,
        init_agent_path: tmp.path(),
    }
    .eval_agent("c", &tasks)
    .unwrap_err();
    assert!(e.starts_with("Dataset file not found: "), "{e}");
}

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

#[cfg(unix)]
#[test]
fn a_new_run_prepares_and_evaluates_the_initial_agent_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    agent_source(root);
    let out = root.join("out");
    let tasks = vec!["a".to_owned(), "b".to_owned()];
    let bench = FakeBench::resolving(&["a"]);
    let docker = Docker::new("true");
    let start = initialize_run(
        &init(root, &out, &tasks, None),
        &bench,
        &docker,
        &Log::discard(),
    )
    .unwrap();
    assert_eq!(start.src_path, root.join("initial_swe/agent/src"));
    assert!(root.join("initial_swe/agent/src/coding_agent.py").is_file());
    assert!(root.join("initial_swe/agent/src/Dockerfile").is_file());
    assert!(out.join("initial/src/coding_agent.py").is_file());
    let seen = bench.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (
            seen[0].commit.as_str(),
            seen[0].tasks.clone(),
            seen[0].max_workers
        ),
        ("agent", tasks.clone(), 2)
    );
    assert_eq!(start.tree.len(), 1);
    assert_eq!(start.tree.get(0).unwrap().utility_measures, [1.0, 0.0]);
    assert_eq!(start.submitted_ids[&0].len(), 2);
    assert_eq!(start.n_task_evals, 0);
    assert!(start.init_evaluated_tasks.is_empty());
    let again = initialize_run(
        &init(root, &out, &tasks, None),
        &bench,
        &docker,
        &Log::discard(),
    )
    .unwrap();
    assert_eq!(again.tree.get(0).unwrap().utility_measures, [1.0, 0.0]);
    assert_eq!(bench.seen.lock().unwrap().len(), 1);
    assert_eq!(
        read_json(&out.join("initial/metadata.json"))["run_id"],
        "initial"
    );
}

#[test]
fn setting_up_the_initial_agent_can_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("out");
    let tasks = vec!["a".to_owned()];
    let bench = FakeBench::resolving(&[]);
    let docker = Docker::new("hgm-no-such-docker-program");
    assert!(
        initialize_run(
            &init(root, &out, &tasks, None),
            &bench,
            &docker,
            &Log::discard()
        )
        .is_err()
    );
    let failing = FakeBench {
        fail: true,
        ..FakeBench::resolving(&[])
    };
    let other = tmp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    agent_source(&other);
    let docker = Docker::new("true");
    #[cfg(unix)]
    assert_eq!(
        initialize_run(
            &init(&other, &out, &tasks, None),
            &failing,
            &docker,
            &Log::discard()
        )
        .err(),
        Some("bench failed".to_owned())
    );
    let fresh = tmp.path().join("fresh");
    fs::create_dir_all(fresh.join("initial_swe/agent")).unwrap();
    let e = initialize_run(
        &init(&fresh, &out, &tasks, None),
        &bench,
        &docker,
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

#[test]
fn a_continued_run_rebuilds_the_tree_from_the_last_record() {
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
    let tasks = vec!["a".to_owned(), "b".to_owned()];
    let bench = FakeBench::resolving(&[]);
    let docker = Docker::new("hgm-no-such-docker-program");
    let start = initialize_run(
        &init(root, &out, &tasks, Some(&out)),
        &bench,
        &docker,
        &Log::discard(),
    )
    .unwrap();
    assert!(bench.seen.lock().unwrap().is_empty());
    assert_eq!(start.tree.len(), 3);
    assert_eq!(start.tree.get(2).unwrap().parent_id, Some(1));
    assert_eq!(start.tree.get(0).unwrap().children, [1]);
    assert_eq!(start.tree.get(2).unwrap().utility_measures, [1.0, 0.0]);
    assert_eq!(start.n_task_evals, 3);
    assert_eq!(start.init_evaluated_tasks, ["a"]);
    assert_eq!(start.submitted_ids[&1].len(), 1);
}

#[test]
fn a_continued_run_needs_its_records() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("run");
    fs::create_dir_all(&out).unwrap();
    let tasks = vec!["a".to_owned()];
    let bench = FakeBench::resolving(&[]);
    let docker = Docker::new("hgm-no-such-docker-program");
    let go = |prev: &Path| {
        initialize_run(
            &init(root, &out, &tasks, Some(prev)),
            &bench,
            &docker,
            &Log::discard(),
        )
    };
    assert!(
        go(&out)
            .err()
            .unwrap()
            .contains("init_evaluated_tasks.json")
    );
    fs::write(out.join("init_evaluated_tasks.json"), "[]").unwrap();
    assert!(go(&out).err().unwrap().contains("hgm_metadata.jsonl"));
    fs::write(out.join("hgm_metadata.jsonl"), "{\"nodes\": [{\"id\": 1}]}").unwrap();
    assert!(go(&out).is_err());
    previous(&out);
    node(&out, "initial", None, performance(&[], &[], &[]));
    assert!(go(&out).err().unwrap().contains("c1"));
}

fn run_args(root: &Path, extra: impl FnOnce(&mut RunArgs)) -> RunArgs {
    let mut args = RunArgs {
        config: root.join("none.yaml"),
        initial_agent_name: "agent".into(),
        ..RunArgs::default()
    };
    extra(&mut args);
    args
}

#[test]
fn a_run_without_an_agent_name_or_output_directory_stops_early() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let unnamed = run_args(root, |a| a.initial_agent_name = String::new());
    assert!(
        run(&unnamed, root, Duration::ZERO)
            .unwrap_err()
            .starts_with("Initial agent name must be provided")
    );
    fs::write(root.join("file"), "").unwrap();
    let under_file = run_args(root, |a| a.output_dir = Some("file/out".into()));
    assert!(run(&under_file, root, Duration::ZERO).is_err());
    fs::create_dir_all(root.join("out/hgm_outer.log")).unwrap();
    let no_log = run_args(root, |a| a.output_dir = Some("out".into()));
    assert!(
        run(&no_log, root, Duration::ZERO)
            .unwrap_err()
            .contains("hgm_outer.log")
    );
}
