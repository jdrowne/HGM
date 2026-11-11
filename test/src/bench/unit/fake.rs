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

fn read_specs() -> (tempfile::TempDir, Vec<hgm_bench::swebench::TestSpec>) {
    let tmp = tempfile::tempdir().unwrap();
    let swebench = python(
        tmp.path(),
        &format!("cat > \"$(dirname \"$0\")/stdin\"\necho '{SPEC}'\necho"),
    );
    let specs = swebench
        .specs(&[json!({"instance_id": "i"}), json!({"x": 1})])
        .unwrap();
    (tmp, specs)
}

#[test]
fn specs_are_read_one_json_line_per_entry() {
    assert_eq!(read_specs().1.len(), 1);
}

#[test]
fn specs_carry_their_image() {
    assert_eq!(read_specs().1[0].image, "img");
}

#[test]
fn specs_send_one_json_line_per_instance() {
    let (tmp, _) = read_specs();
    assert_eq!(
        fs::read_to_string(tmp.path().join("stdin")).unwrap(),
        "{\"instance_id\":\"i\"}\n{\"x\":1}\n"
    );
}

#[test]
fn a_failing_spec_run_reports_its_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let failing = python(tmp.path(), "echo ' no module ' >&2\nexit 1");
    assert_eq!(
        failing.specs(&[json!({})]).unwrap_err().to_string(),
        "swebench test specs failed: no module"
    );
}

fn garbled() -> Error {
    let tmp = tempfile::tempdir().unwrap();
    python(tmp.path(), "echo nope").specs(&[]).unwrap_err()
}

#[test]
fn a_garbled_spec_run_is_an_output_error() {
    let e = garbled();
    assert!(matches!(e, Error::Output(_)), "{e}");
}

#[test]
fn a_garbled_spec_run_says_so() {
    let e = garbled().to_string();
    assert!(e.starts_with("unexpected swebench output: "), "{e}");
}

fn recording(dir: &Path) -> Swebench {
    python(
        dir,
        "echo \"$@\" > \"$(dirname \"$0\")/args\"\npwd >> \"$(dirname \"$0\")/args\"",
    )
}

#[test]
fn preparing_images_passes_the_instances() {
    let tmp = tempfile::tempdir().unwrap();
    recording(tmp.path())
        .prepare_images("d", &["a", "b"], 3)
        .unwrap();
    let args = fs::read_to_string(tmp.path().join("args")).unwrap();
    assert!(args.starts_with("-m swebench.harness.prepare_images --dataset_name d --split test --max_workers 3 --instance_ids a b\n"), "{args}");
}

#[test]
fn evaluations_run_in_the_output_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    fs::create_dir(&out).unwrap();
    recording(tmp.path())
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
}

#[test]
fn a_failing_image_build_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let e = python(tmp.path(), "exit 4")
        .prepare_images("d", &["a"], 1)
        .unwrap_err()
        .to_string();
    assert!(e.starts_with("swebench image build failed: "), "{e}");
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

fn runs_python(dir: &Path) -> Swebench {
    python(dir, "echo \"$@\" >> \"$(dirname \"$0\")/runs\"")
}

fn ids() -> [String; 2] {
    ["x".to_owned(), "y".to_owned()]
}

struct Made {
    tmp: tempfile::TempDir,
    results: Vec<Result<(), String>>,
}

impl Made {
    fn runs(&self) -> String {
        fs::read_to_string(self.tmp.path().join("runs")).unwrap()
    }
}

fn both() -> Made {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = [
        prediction_dir(tmp.path(), "a"),
        prediction_dir(tmp.path(), "b"),
    ];
    let report = Report {
        output_dir: tmp.path().to_path_buf(),
        dnames_workers: Some(1),
        ..Report::default()
    };
    let results = report.make(&runs_python(tmp.path()), &dirs, Some(&ids()));
    Made { tmp, results }
}

#[test]
fn reports_run_one_evaluation_per_prediction_directory() {
    let made = both();
    assert!(made.results.iter().all(Result::is_ok), "{:?}", made.results);
}

#[test]
fn the_first_evaluation_takes_the_first_run_id() {
    let runs = both().runs();
    assert!(runs.contains("--run_id x"), "{runs}");
}

#[test]
fn the_second_evaluation_takes_the_second_run_id() {
    let runs = both().runs();
    assert!(runs.contains("--run_id y"), "{runs}");
}

#[test]
fn reports_write_each_directory_s_predictions() {
    assert!(both().tmp.path().join("a/all_preds.jsonl").is_file());
}

fn mixed() -> Made {
    let tmp = tempfile::tempdir().unwrap();
    let a = prediction_dir(tmp.path(), "a");
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let report = Report {
        output_dir: tmp.path().to_path_buf(),
        ..Report::default()
    };
    let results = report.make(
        &runs_python(tmp.path()),
        &[a, tmp.path().join("missing"), PathBuf::new(), empty],
        Some(&ids()),
    );
    Made { tmp, results }
}

#[test]
fn a_directory_with_predictions_is_evaluated() {
    let made = mixed();
    assert!(made.results[0].is_ok(), "{:?}", made.results);
}

#[test]
fn a_missing_directory_does_not_exist() {
    assert!(
        mixed().results[1]
            .as_ref()
            .unwrap_err()
            .ends_with("does not exist")
    );
}

#[test]
fn an_empty_path_is_an_error() {
    assert!(mixed().results[2].is_err());
}

#[test]
fn a_directory_without_predictions_is_an_error() {
    assert!(
        mixed().results[3]
            .as_ref()
            .unwrap_err()
            .starts_with("no predictions in")
    );
}

#[test]
fn a_serial_report_uses_the_default_run_ids() {
    let runs = mixed().runs();
    assert!(runs.contains("--run_id 000"), "{runs}");
}

#[test]
fn a_failing_evaluation_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let b = prediction_dir(tmp.path(), "b");
    let report = Report {
        output_dir: tmp.path().to_path_buf(),
        ..Report::default()
    };
    let results = report.make(&python(tmp.path(), "exit 1"), &[b], None);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .starts_with("swebench evaluation failed")
    );
}
