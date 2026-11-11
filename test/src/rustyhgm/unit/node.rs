use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm::agent_build::Build;
use rustyhgm::node::{Bench, Eval, Evaluator, empty_metadata, load_metadata, put, save_metadata};
use rustyhgm_bench::dataset::Hub;
use rustyhgm_bench::polyglot;
use rustyhgm_bench::swe;
use rustyhgm_bench::swebench::Swebench;
use rustyhgm_core::pyjson::PyValue;
use rustyhgm_sandbox::Docker;
use serde_json::json;

use crate::common::{FakeBench, FakeBuild, node, performance, read_json, service, write_json};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn eval(evaluator: Evaluator<'_>, commit: &str, tasks: &[&str]) -> Result<Vec<f64>, String> {
    evaluator.eval_agent(commit, &strings(tasks))
}

fn evaluator<'a>(
    bench: &'a dyn Bench,
    out: &'a Path,
    max_workers: usize,
    builder: &'a dyn Build,
) -> Evaluator<'a> {
    Evaluator {
        bench,
        output_dir: out,
        max_workers,
        builder,
    }
}

fn put_twice() -> rustyhgm::node::Metadata {
    let mut m = empty_metadata("c");
    put(&mut m, "run_id", PyValue::str("d"));
    put(&mut m, "extra", PyValue::Int(1));
    m
}

#[test]
fn put_appends_new_keys() {
    let m = put_twice();
    let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["run_id", "overall_performance", "extra"]);
}

#[test]
fn put_replaces_in_place() {
    assert_eq!(put_twice()[0].1, PyValue::str("d"));
}

fn saved() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    save_metadata(tmp.path(), &empty_metadata("abc")).unwrap();
    tmp
}

#[test]
fn metadata_is_written_as_python_writes_it() {
    let text = fs::read_to_string(saved().path().join("metadata.json")).unwrap();
    assert!(text.starts_with("{\n    \"run_id\": \"abc\",\n    \"overall_performance\": {\n        \"accuracy_score\": 0,"), "{text}");
}

#[test]
fn metadata_round_trips() {
    assert_eq!(
        load_metadata(saved().path()).unwrap(),
        empty_metadata("abc")
    );
}

#[test]
fn missing_metadata_is_an_error_naming_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let e = load_metadata(tmp.path()).unwrap_err();
    assert!(e.contains("metadata.json"), "{e}");
}

fn metadata_of(text: &str) -> Result<rustyhgm::node::Metadata, String> {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("metadata.json"), text).unwrap();
    load_metadata(tmp.path())
}

#[test]
fn junk_metadata_is_an_error() {
    assert!(metadata_of("{").is_err());
}

#[test]
fn metadata_that_is_not_an_object_is_an_error() {
    assert!(
        metadata_of("[]")
            .unwrap_err()
            .ends_with("not a JSON object")
    );
}

#[test]
fn saving_into_a_missing_folder_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(save_metadata(&tmp.path().join("missing"), &empty_metadata("c")).is_err());
}

struct Evaluated {
    tmp: tempfile::TempDir,
    bench: FakeBench,
    builder: FakeBuild,
    accs: Vec<f64>,
}

impl Evaluated {
    fn out(&self) -> &Path {
        self.tmp.path()
    }

    fn seen(&self) -> Vec<crate::common::Seen> {
        self.bench.seen.lock().unwrap().clone()
    }

    fn metadata(&self) -> serde_json::Value {
        read_json(&self.out().join("child/metadata.json"))
    }
}

fn evaluated() -> Evaluated {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    node(out, "parent", None, performance(&[], &[], &[]));
    fs::write(out.join("parent/model_patch.diff"), "p").unwrap();
    node(out, "child", Some("parent"), performance(&[], &[], &[]));
    fs::write(out.join("child/model_patch.diff"), "c").unwrap();
    let bench = FakeBench::resolving(&["a"]);
    let builder = FakeBuild::default();
    let accs = eval(evaluator(&bench, out, 8, &builder), "child", &["a", "b"]).unwrap();
    fs::write(out.join("parent/rustyhgm-coding-agent"), "").unwrap();
    eval(evaluator(&bench, out, 8, &builder), "parent", &["a", "b"]).unwrap();
    Evaluated {
        tmp,
        bench,
        builder,
        accs,
    }
}

#[test]
fn eval_agent_returns_per_task_results() {
    assert_eq!(evaluated().accs, [1.0, 0.0]);
}

#[test]
fn eval_agent_builds_a_missing_agent_from_the_patch_chain() {
    let e = evaluated();
    let out = e.out();
    assert_eq!(
        *e.builder.built.lock().unwrap(),
        [(
            vec![
                out.join("parent/model_patch.diff"),
                out.join("child/model_patch.diff")
            ],
            out.join("child/rustyhgm-coding-agent")
        )]
    );
}

#[test]
fn eval_agent_runs_the_bench_once_per_call() {
    assert_eq!(evaluated().seen().len(), 2);
}

#[test]
fn eval_agent_names_the_commit() {
    assert_eq!(evaluated().seen()[0].commit, "child");
}

#[test]
fn eval_agent_caps_workers_at_the_task_count() {
    assert_eq!(evaluated().seen()[0].max_workers, 2);
}

#[test]
fn eval_agent_runs_the_built_agent() {
    let e = evaluated();
    assert_eq!(
        e.seen()[0].agent,
        e.out().join("child/rustyhgm-coding-agent")
    );
}

#[test]
fn eval_agent_runs_an_existing_agent() {
    let e = evaluated();
    assert_eq!(
        e.seen()[1].agent,
        e.out().join("parent/rustyhgm-coding-agent")
    );
}

#[test]
fn eval_agent_keeps_the_parent_commit() {
    assert_eq!(evaluated().metadata()["parent_commit"], "parent");
}

#[test]
fn eval_agent_records_the_resolved_tasks() {
    assert_eq!(
        evaluated().metadata()["overall_performance"]["total_resolved_ids"],
        json!(["a"])
    );
}

#[test]
fn eval_agent_records_the_accuracy() {
    assert_eq!(
        evaluated().metadata()["overall_performance"]["accuracy_score"],
        0.5
    );
}

#[test]
fn eval_agent_records_no_swe_dnames_without_them() {
    assert!(evaluated().metadata().get("swe_dnames").is_none());
}

fn fresh_agent() -> (tempfile::TempDir, Vec<f64>) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    fs::create_dir_all(out.join("agent")).unwrap();
    let mut bench = FakeBench::resolving(&[]);
    bench.dnames = Some(vec![PathBuf::from("/p/agent_0")]);
    let accs = eval(
        evaluator(&bench, out, 1, &FakeBuild::default()),
        "agent",
        &["a"],
    )
    .unwrap();
    (tmp, accs)
}

fn fresh_metadata() -> serde_json::Value {
    let (tmp, _) = fresh_agent();
    read_json(&tmp.path().join("agent/metadata.json"))
}

#[test]
fn eval_agent_without_metadata_still_evaluates() {
    assert_eq!(fresh_agent().1, [0.0]);
}

#[test]
fn eval_agent_creates_missing_metadata() {
    let metadata = fresh_metadata();
    let keys: Vec<&String> = metadata.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["run_id", "overall_performance", "swe_dnames"]);
}

#[test]
fn created_metadata_names_the_commit() {
    assert_eq!(fresh_metadata()["run_id"], "agent");
}

#[test]
fn eval_agent_records_swe_dnames() {
    assert_eq!(fresh_metadata()["swe_dnames"], json!(["/p/agent_0"]));
}

#[test]
fn created_metadata_records_the_submitted_tasks() {
    assert_eq!(
        fresh_metadata()["overall_performance"]["total_submitted_ids"],
        json!(["a"])
    );
}

#[test]
fn evaluating_no_tasks_returns_nothing() {
    let (tmp, _) = fresh_agent();
    let bench = FakeBench::resolving(&[]);
    assert_eq!(
        eval(
            evaluator(&bench, tmp.path(), 4, &FakeBuild::default()),
            "agent",
            &[]
        )
        .unwrap(),
        Vec::<f64>::new()
    );
}

fn eval_fails(
    bench: &dyn Bench,
    builder: &dyn Build,
    prepare: impl FnOnce(&Path),
    commit: &str,
) -> Result<Vec<f64>, String> {
    let tmp = tempfile::tempdir().unwrap();
    prepare(tmp.path());
    eval(evaluator(bench, tmp.path(), 1, builder), commit, &["a"])
}

fn plain(out: &Path) {
    node(out, "ok", None, performance(&[], &[], &[]));
}

#[test]
fn a_missing_node_folder_is_an_error() {
    let bench = FakeBench::resolving(&[]);
    assert!(eval_fails(&bench, &FakeBuild::default(), |_| {}, "nodir").is_err());
}

#[test]
fn junk_node_metadata_is_an_error() {
    let bench = FakeBench::resolving(&[]);
    let junk = |out: &Path| {
        fs::create_dir_all(out.join("junk")).unwrap();
        fs::write(out.join("junk/metadata.json"), "[]").unwrap();
    };
    assert!(eval_fails(&bench, &FakeBuild::default(), junk, "junk").is_err());
}

#[test]
fn a_node_whose_parent_is_gone_is_an_error() {
    let bench = FakeBench::resolving(&[]);
    let orphan = |out: &Path| node(out, "orphan", Some("gone"), performance(&[], &[], &[]));
    assert!(eval_fails(&bench, &FakeBuild::default(), orphan, "orphan").is_err());
}

#[test]
fn a_failing_agent_build_is_reported() {
    let bench = FakeBench::resolving(&[]);
    let broken = FakeBuild {
        fail: true,
        ..FakeBuild::default()
    };
    assert_eq!(
        eval_fails(&bench, &broken, plain, "ok").unwrap_err(),
        "build failed"
    );
}

#[test]
fn a_failing_bench_is_reported() {
    let failing = FakeBench {
        fail: true,
        ..FakeBench::resolving(&[])
    };
    assert_eq!(
        eval_fails(&failing, &FakeBuild::default(), plain, "ok").unwrap_err(),
        "bench failed"
    );
}

struct Vanish;

impl Bench for Vanish {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
        fs::remove_dir_all(eval.node_dir).unwrap();
        Ok(None)
    }
}

#[test]
fn a_node_folder_that_vanishes_is_an_error() {
    assert!(eval_fails(&Vanish, &FakeBuild::default(), plain, "ok").is_err());
}

struct Block;

impl Bench for Block {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
        let metadata = eval.node_dir.join("metadata.json");
        fs::remove_file(&metadata).unwrap();
        fs::create_dir(&metadata).unwrap();
        Ok(None)
    }
}

#[test]
fn unwritable_metadata_after_the_bench_is_an_error() {
    assert!(eval_fails(&Block, &FakeBuild::default(), plain, "ok").is_err());
}

fn polyglot_harness(root: &Path) -> polyglot::Harness {
    write_json(&root.join("meta.json"), &json!([]));
    fs::create_dir_all(root.join("out/c")).unwrap();
    polyglot::Harness {
        docker: Docker::new("rustyhgm-no-such-docker-program"),
        root: root.to_path_buf(),
        dataset: root.join("meta.json"),
        machine: "x86_64".into(),
        namespace: "pb".into(),
        llm: "m".into(),
        service: service(),
        timeout: 1,
    }
}

fn polyglot_run() -> (tempfile::TempDir, Vec<f64>) {
    let tmp = tempfile::tempdir().unwrap();
    let harness = polyglot_harness(tmp.path());
    let accs = eval(
        evaluator(&harness, &tmp.path().join("out"), 2, &FakeBuild::default()),
        "c",
        &["a"],
    )
    .unwrap();
    (tmp, accs)
}

#[test]
fn the_polyglot_bench_scores_an_unknown_task_as_unresolved() {
    assert_eq!(polyglot_run().1, [0.0]);
}

#[test]
fn the_polyglot_bench_writes_its_predictions() {
    assert!(
        polyglot_run()
            .0
            .path()
            .join("out/c/predictions/c_0")
            .is_dir()
    );
}

#[test]
fn the_polyglot_bench_records_the_submissions() {
    let (tmp, _) = polyglot_run();
    assert_eq!(
        read_json(&tmp.path().join("out/c/metadata.json"))["overall_performance"]["total_submitted_instances"],
        0
    );
}

#[test]
fn the_polyglot_bench_fails_without_its_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = polyglot::Harness {
        dataset: tmp.path().join("none.json"),
        ..polyglot_harness(tmp.path())
    };
    assert!(
        eval(
            evaluator(&missing, &tmp.path().join("out"), 2, &FakeBuild::default()),
            "c",
            &["a"]
        )
        .is_err()
    );
}

#[test]
fn the_swe_bench_fails_when_the_dataset_cannot_load() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let harness = swe::Harness {
        docker: Docker::new("rustyhgm-no-such-docker-program"),
        swebench: Swebench {
            python: "rustyhgm-no-such-python".into(),
        },
        hub: Hub {
            endpoint: "http://127.0.0.1:9".into(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        service: service(),
        timeout: 1,
    };
    fs::create_dir_all(root.join("out/c")).unwrap();
    assert!(
        eval(
            evaluator(&harness, &root.join("out"), 2, &FakeBuild::default()),
            "c",
            &["a"]
        )
        .is_err()
    );
}

#[cfg(unix)]
fn swe_run() -> (tempfile::TempDir, Vec<f64>) {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let cache = root.join("cache/princeton-nlp/SWE-bench/test");
    fs::create_dir_all(&cache).unwrap();
    let fixture = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures/rows.parquet");
    fs::copy(fixture, cache.join("rows.parquet")).unwrap();
    fs::write(cache.join("files.json"), r#"["rows.parquet"]"#).unwrap();
    let python = root.join("python");
    fs::write(
        &python,
        "#!/bin/sh\n[ \"$1\" = -m ] && exit 0\ncat > /dev/null\n",
    )
    .unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    let harness = swe::Harness {
        docker: Docker::new("rustyhgm-no-such-docker-program"),
        swebench: Swebench {
            python: python.to_string_lossy().into_owned(),
        },
        hub: Hub {
            endpoint: "http://127.0.0.1:9".into(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        service: service(),
        timeout: 1,
    };
    let out = root.join("out");
    fs::create_dir_all(out.join("c")).unwrap();
    let accs = eval(
        evaluator(&harness, &out, 2, &FakeBuild::default()),
        "c",
        &["absent"],
    )
    .unwrap();
    (tmp, accs)
}

#[cfg(unix)]
#[test]
fn the_swe_bench_scores_an_absent_task_as_unresolved() {
    assert_eq!(swe_run().1, [0.0]);
}

#[cfg(unix)]
#[test]
fn the_swe_bench_runs_the_harness_then_the_report() {
    let (tmp, _) = swe_run();
    let out = tmp.path().join("out");
    assert_eq!(
        read_json(&out.join("c/metadata.json"))["swe_dnames"],
        json!([out.join("c/predictions/c_1").display().to_string()])
    );
}
