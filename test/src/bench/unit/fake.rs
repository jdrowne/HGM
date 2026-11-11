use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use hgm_bench::report::Report;
use hgm_bench::swebench::{Error, EvaluationRun, Swebench};
use serde_json::json;

pub fn python(dir: &Path, body: &str) -> Swebench {
    let path = dir.join("python");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Swebench {
        python: path.to_string_lossy().into_owned(),
    }
}

const SPEC: &str = r#"{"instance_id": "i", "eval_script": "e", "image": "img", "platform": "p", "user": "root", "nano_cpus": null}"#;

#[test]
fn specs_are_read_one_json_line_per_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let swebench = python(
        tmp.path(),
        &format!("cat > \"$(dirname \"$0\")/stdin\"\necho '{SPEC}'\necho"),
    );
    let specs = swebench
        .specs(&[json!({"instance_id": "i"}), json!({"x": 1})])
        .unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].image, "img");
    assert_eq!(
        fs::read_to_string(tmp.path().join("stdin")).unwrap(),
        "{\"instance_id\":\"i\"}\n{\"x\":1}\n"
    );
}

#[test]
fn a_failing_or_garbled_spec_run_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let failing = python(tmp.path(), "echo ' no module ' >&2\nexit 1");
    let e = failing.specs(&[json!({})]).unwrap_err();
    assert_eq!(e.to_string(), "swebench test specs failed: no module");
    let garbled = python(tmp.path(), "echo nope");
    let e = garbled.specs(&[]).unwrap_err();
    assert!(matches!(e, Error::Output(_)), "{e}");
    assert!(
        e.to_string().starts_with("unexpected swebench output: "),
        "{e}"
    );
}

#[test]
fn modules_get_their_arguments_and_working_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let swebench = python(
        tmp.path(),
        "echo \"$@\" > \"$(dirname \"$0\")/args\"\npwd >> \"$(dirname \"$0\")/args\"",
    );
    swebench.prepare_images("d", &["a", "b"], 3).unwrap();
    let args = fs::read_to_string(tmp.path().join("args")).unwrap();
    assert!(args.starts_with("-m swebench.harness.prepare_images --dataset_name d --split test --max_workers 3 --instance_ids a b\n"), "{args}");
    let out = tmp.path().join("out");
    fs::create_dir(&out).unwrap();
    swebench
        .run_evaluation(&EvaluationRun {
            dataset_name: "d".into(),
            predictions_path: PathBuf::from("/p.jsonl"),
            max_workers: 2,
            run_id: "r".into(),
            output_dir: out.clone(),
        })
        .unwrap();
    let args = fs::read_to_string(tmp.path().join("args")).unwrap();
    let out = fs::canonicalize(out).unwrap();
    assert_eq!(
        args,
        format!(
            "-m swebench.harness.run_evaluation --dataset_name d --predictions_path /p.jsonl --max_workers 2 --run_id r\n{}\n",
            out.display()
        )
    );
    let failing = python(tmp.path(), "exit 4");
    let e = failing.prepare_images("d", &["a"], 1).unwrap_err();
    assert!(
        e.to_string().starts_with("swebench image build failed: "),
        "{e}"
    );
}

fn prediction_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("i.json"),
        r#"{"instance_id": "i", "model_name_or_path": "m", "model_patch": "p"}"#,
    )
    .unwrap();
    dir
}

#[test]
fn reports_run_one_evaluation_per_prediction_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let swebench = python(tmp.path(), "echo \"$@\" >> \"$(dirname \"$0\")/runs\"");
    let a = prediction_dir(tmp.path(), "a");
    let b = prediction_dir(tmp.path(), "b");
    let report = Report {
        output_dir: tmp.path().to_path_buf(),
        dnames_workers: Some(1),
        ..Report::default()
    };
    let ids = ["x".to_owned(), "y".to_owned()];
    let results = report.make(&swebench, &[a.clone(), b.clone()], Some(&ids));
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let runs = fs::read_to_string(tmp.path().join("runs")).unwrap();
    assert!(
        runs.contains("--run_id x") && runs.contains("--run_id y"),
        "{runs}"
    );
    assert!(a.join("all_preds.jsonl").is_file());
    let report = Report {
        output_dir: tmp.path().to_path_buf(),
        ..Report::default()
    };
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let results = report.make(
        &swebench,
        &[a, tmp.path().join("missing"), PathBuf::new(), empty],
        Some(&ids),
    );
    assert!(results[0].is_ok(), "{results:?}");
    assert!(results[1].as_ref().unwrap_err().ends_with("does not exist"));
    assert!(results[2].is_err());
    assert!(
        results[3]
            .as_ref()
            .unwrap_err()
            .starts_with("no predictions in")
    );
    let runs = fs::read_to_string(tmp.path().join("runs")).unwrap();
    assert!(runs.contains("--run_id 000"), "{runs}");
    let failing = python(tmp.path(), "exit 1");
    let results = report.make(&failing, &[b], None);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .starts_with("swebench evaluation failed")
    );
}
