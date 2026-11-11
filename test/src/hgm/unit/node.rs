use std::fs;
use std::path::PathBuf;

use hgm::node::{Bench, Eval, Evaluator, empty_metadata, load_metadata, put, save_metadata};
use hgm_bench::dataset::Hub;
use hgm_bench::polyglot;
use hgm_bench::swe;
use hgm_bench::swebench::Swebench;
use hgm_core::pyjson::PyValue;
use hgm_sandbox::Docker;
use serde_json::json;

use crate::common::{FakeBench, node, performance, read_json, write_json};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn put_replaces_in_place_or_appends() {
    let mut m = empty_metadata("c");
    put(&mut m, "run_id", PyValue::str("d"));
    put(&mut m, "extra", PyValue::Int(1));
    let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["run_id", "overall_performance", "extra"]);
    assert_eq!(m[0].1, PyValue::str("d"));
}

#[test]
fn metadata_round_trips_as_python_writes_it() {
    let tmp = tempfile::tempdir().unwrap();
    save_metadata(tmp.path(), &empty_metadata("abc")).unwrap();
    let text = fs::read_to_string(tmp.path().join("metadata.json")).unwrap();
    assert!(text.starts_with("{\n    \"run_id\": \"abc\",\n    \"overall_performance\": {\n        \"accuracy_score\": 0,"), "{text}");
    assert_eq!(load_metadata(tmp.path()).unwrap(), empty_metadata("abc"));
}

#[test]
fn unreadable_metadata_is_an_error_naming_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let e = load_metadata(tmp.path()).unwrap_err();
    assert!(e.contains("metadata.json"), "{e}");
    fs::write(tmp.path().join("metadata.json"), "{").unwrap();
    assert!(load_metadata(tmp.path()).is_err());
    fs::write(tmp.path().join("metadata.json"), "[]").unwrap();
    assert!(
        load_metadata(tmp.path())
            .unwrap_err()
            .ends_with("not a JSON object")
    );
    assert!(save_metadata(&tmp.path().join("missing"), &empty_metadata("c")).is_err());
}

#[test]
fn eval_agent_records_performance_and_returns_per_task_results() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    node(out, "parent", None, performance(&[], &[], &[]));
    fs::write(out.join("parent/model_patch.diff"), "p").unwrap();
    node(out, "child", Some("parent"), performance(&[], &[], &[]));
    fs::write(out.join("child/model_patch.diff"), "c").unwrap();
    let bench = FakeBench::resolving(&["a"]);
    let tasks = strings(&["a", "b"]);
    let accs = Evaluator {
        bench: &bench,
        output_dir: out,
        max_workers: 8,
        init_agent_path: &PathBuf::from("src"),
    }
    .eval_agent("child", &tasks)
    .unwrap();
    assert_eq!(accs, [1.0, 0.0]);
    let seen = bench.seen.lock().unwrap();
    assert_eq!(seen[0].commit, "child");
    assert_eq!(seen[0].max_workers, 2);
    assert_eq!(
        seen[0].patches,
        [
            out.join("parent/model_patch.diff"),
            out.join("child/model_patch.diff")
        ]
    );
    let metadata = read_json(&out.join("child/metadata.json"));
    assert_eq!(metadata["parent_commit"], "parent");
    assert_eq!(
        metadata["overall_performance"]["total_resolved_ids"],
        json!(["a"])
    );
    assert_eq!(metadata["overall_performance"]["accuracy_score"], 0.5);
    assert!(metadata.get("swe_dnames").is_none());
}

#[test]
fn eval_agent_creates_missing_metadata_and_records_swe_dnames() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    fs::create_dir_all(out.join("agent")).unwrap();
    let mut bench = FakeBench::resolving(&[]);
    bench.dnames = Some(vec![PathBuf::from("/p/agent_0")]);
    assert_eq!(
        Evaluator {
            bench: &bench,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &PathBuf::from("src")
        }
        .eval_agent("agent", &strings(&["a"]))
        .unwrap(),
        [0.0]
    );
    let metadata = read_json(&out.join("agent/metadata.json"));
    let keys: Vec<&String> = metadata.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["run_id", "overall_performance", "swe_dnames"]);
    assert_eq!(metadata["run_id"], "agent");
    assert_eq!(metadata["swe_dnames"], json!(["/p/agent_0"]));
    assert_eq!(
        metadata["overall_performance"]["total_submitted_ids"],
        json!(["a"])
    );
    assert_eq!(
        Evaluator {
            bench: &bench,
            output_dir: out,
            max_workers: 4,
            init_agent_path: &PathBuf::from("src")
        }
        .eval_agent("agent", &[])
        .unwrap(),
        Vec::<f64>::new()
    );
}

#[test]
fn eval_agent_stops_at_the_first_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    let bench = FakeBench::resolving(&[]);
    let tasks = strings(&["a"]);
    let src = PathBuf::from("src");
    assert!(
        Evaluator {
            bench: &bench,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("nodir", &tasks)
        .is_err()
    );
    fs::create_dir_all(out.join("junk")).unwrap();
    fs::write(out.join("junk/metadata.json"), "[]").unwrap();
    assert!(
        Evaluator {
            bench: &bench,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("junk", &tasks)
        .is_err()
    );
    node(out, "orphan", Some("gone"), performance(&[], &[], &[]));
    assert!(
        Evaluator {
            bench: &bench,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("orphan", &tasks)
        .is_err()
    );
    node(out, "ok", None, performance(&[], &[], &[]));
    let failing = FakeBench {
        fail: true,
        ..FakeBench::resolving(&[])
    };
    assert_eq!(
        Evaluator {
            bench: &failing,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("ok", &tasks)
        .unwrap_err(),
        "bench failed"
    );
    struct Vanish;
    impl Bench for Vanish {
        fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
            fs::remove_dir_all(eval.node_dir).unwrap();
            Ok(None)
        }
    }
    assert!(
        Evaluator {
            bench: &Vanish,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("ok", &tasks)
        .is_err()
    );
    struct Block;
    impl Bench for Block {
        fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
            let metadata = eval.node_dir.join("metadata.json");
            fs::remove_file(&metadata).unwrap();
            fs::create_dir(&metadata).unwrap();
            Ok(None)
        }
    }
    node(out, "blocked", None, performance(&[], &[], &[]));
    assert!(
        Evaluator {
            bench: &Block,
            output_dir: out,
            max_workers: 1,
            init_agent_path: &src
        }
        .eval_agent("blocked", &tasks)
        .is_err()
    );
}

#[test]
fn the_polyglot_bench_runs_the_polyglot_harness() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_json(&root.join("meta.json"), &json!([]));
    let harness = polyglot::Harness {
        docker: Docker::new("hgm-no-such-docker-program"),
        root: root.to_path_buf(),
        dataset: root.join("meta.json"),
        machine: "x86_64".into(),
        namespace: "pb".into(),
        llm: "m".into(),
        timeout: 1,
    };
    fs::create_dir_all(root.join("out/c")).unwrap();
    let accs = Evaluator {
        bench: &harness,
        output_dir: &root.join("out"),
        max_workers: 2,
        init_agent_path: root,
    }
    .eval_agent("c", &strings(&["a"]))
    .unwrap();
    assert_eq!(accs, [0.0]);
    assert!(root.join("out/c/predictions/c_0").is_dir());
    let metadata = read_json(&root.join("out/c/metadata.json"));
    assert_eq!(
        metadata["overall_performance"]["total_submitted_instances"],
        0
    );
    let missing = polyglot::Harness {
        dataset: root.join("none.json"),
        ..harness
    };
    assert!(
        Evaluator {
            bench: &missing,
            output_dir: &root.join("out"),
            max_workers: 2,
            init_agent_path: root
        }
        .eval_agent("c", &strings(&["a"]))
        .is_err()
    );
}

#[test]
fn the_swe_bench_fails_when_the_dataset_cannot_load() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let harness = swe::Harness {
        docker: Docker::new("hgm-no-such-docker-program"),
        swebench: Swebench {
            python: "hgm-no-such-python".into(),
        },
        hub: Hub {
            endpoint: "http://127.0.0.1:9".into(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        timeout: 1,
    };
    fs::create_dir_all(root.join("out/c")).unwrap();
    assert!(
        Evaluator {
            bench: &harness,
            output_dir: &root.join("out"),
            max_workers: 2,
            init_agent_path: root
        }
        .eval_agent("c", &strings(&["a"]))
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn the_swe_bench_runs_the_harness_then_the_report() {
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
        docker: Docker::new("hgm-no-such-docker-program"),
        swebench: Swebench {
            python: python.to_string_lossy().into_owned(),
        },
        hub: Hub {
            endpoint: "http://127.0.0.1:9".into(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        timeout: 1,
    };
    let out = root.join("out");
    fs::create_dir_all(out.join("c")).unwrap();
    let accs = Evaluator {
        bench: &harness,
        output_dir: &out,
        max_workers: 2,
        init_agent_path: root,
    }
    .eval_agent("c", &strings(&["absent"]))
    .unwrap();
    assert_eq!(accs, [0.0]);
    let metadata = read_json(&out.join("c/metadata.json"));
    assert_eq!(
        metadata["swe_dnames"],
        json!([out.join("c/predictions/c_1").display().to_string()])
    );
}
