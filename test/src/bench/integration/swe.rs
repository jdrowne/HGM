use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustyhgm_bench::dataset::{Hub, SWE_BENCH};
use rustyhgm_bench::swe::{EntryResult, Evaluator, Run};
use rustyhgm_bench::swebench::{Swebench, TestSpec};
use rustyhgm_sandbox::Docker;
use serde_json::{Value, json};

const IMAGE: &str = "rustyhgm-bench-fake-swe:latest";

const HARNESS: &str = r#"#!/bin/sh
mode=$(cat /mode 2>/dev/null)
while [ $# -gt 0 ]; do
  case "$1" in
    --instance_id) id=$2; shift ;;
    --chat_history_file) chat=$2; shift ;;
    --api_key_file) key=$2; shift ;;
  esac
  shift
done
case "$mode" in
  fail) echo harness failed; exit 1 ;;
  norustyhgm) rm -rf /rustyhgm; exit 0 ;;
esac
[ "$mode" = nochat ] || echo "chat $id $(cat "$key")" > "$chat"
echo "more $id" > "/rustyhgm/${id}_1.md"
[ "$mode" = nopatch ] || echo base > /rustyhgm/model_patch.diff
echo "proposal $id" > /rustyhgm/model_patch_1.diff
"#;

fn platform() -> String {
    let out = Command::new("docker")
        .args(["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    out.unwrap_or_default()
}

fn image() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Dockerfile"),
        "FROM busybox:latest\nCOPY bash /bin/bash\nRUN chmod 755 /bin/bash\n",
    )
    .unwrap();
    fs::write(dir.path().join("bash"), "#!/bin/sh\nexec /bin/sh \"$@\"\n").unwrap();
    let status = Command::new("docker")
        .args(["build", "-q", "-t", IMAGE])
        .arg(dir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    if !status.success() {
        panic!("building {IMAGE} failed");
    }
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    crate::executable(&path, &format!("#!/bin/sh\n{body}\n"));
    path.to_string_lossy().into_owned()
}

fn spec_line(image: &str) -> String {
    format!(
        r#"sed -n 's|.*"instance_id":"\([^"]*\)".*|{{"instance_id": "\1", "eval_script": "true", "image": "{image}", "platform": "{}", "user": "root", "nano_cpus": 1000000000}}|p'"#,
        platform()
    )
}

fn swebench(dir: &Path, prepare: &str, specs: &str) -> Swebench {
    let body = format!("if [ \"$1\" = -m ]; then\n{prepare}\nelse\n{specs}\nfi");
    Swebench {
        python: script(dir, "python", &body),
    }
}

fn evaluator(root: &Path, swebench: Swebench) -> Evaluator {
    Evaluator {
        docker: Docker::new("docker"),
        swebench,
        hub: Hub {
            endpoint: refused(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        service: crate::service(root),
        timeout: 60,
    }
}

fn refused() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn harness_run(root: &Path) -> Run {
    let dir = root.join("harness");
    fs::create_dir_all(&dir).unwrap();
    Run {
        model_name_or_path: "harness".into(),
        harness: PathBuf::from(script(
            &dir,
            "rustyhgm-coding-harness",
            HARNESS.trim_start_matches("#!/bin/sh\n"),
        )),
        max_workers: 4,
    }
}

fn cache_rows(root: &Path) {
    let dir = root.join("cache").join(SWE_BENCH).join("test");
    fs::create_dir_all(&dir).unwrap();
    let fixture = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures/rows.parquet");
    fs::copy(fixture, dir.join("rows.parquet")).unwrap();
    fs::write(dir.join("files.json"), r#"["rows.parquet"]"#).unwrap();
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

const TASKS: [&str; 3] = ["astropy__astropy-12907", "django__django-11099", "absent"];

struct Full {
    tmp: tempfile::TempDir,
    h: Evaluator,
    pred: PathBuf,
    dirs: Vec<PathBuf>,
}

impl Full {
    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn per_task<T>(&self, pick: impl Fn(&Path, &str) -> T) -> Vec<T> {
        TASKS[..2]
            .iter()
            .map(|id| pick(&self.dirs[0], id))
            .collect()
    }

    fn again(&self) -> Vec<PathBuf> {
        self.h
            .run_tasks(&TASKS[..1], &harness_run(self.root()), &self.pred)
            .unwrap()
    }
}

fn full() -> Full {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    cache_rows(root);
    let h = evaluator(root, swebench(root, "exit 0", &spec_line(IMAGE)));
    let pred = root.join("pred");
    let dirs = h.run_tasks(&TASKS, &harness_run(root), &pred).unwrap();
    Full { tmp, h, pred, dirs }
}

fn prediction(dir: &Path, id: &str) -> Value {
    read(&dir.join(format!("{id}.json")))
}

fn text(path: PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn a_full_run_writes_one_prediction_directory() {
    let f = full();
    assert_eq!(f.dirs, [f.pred.join("harness_1")]);
}

#[test]
fn predictions_name_the_run() {
    assert_eq!(
        full().per_task(|dir, id| prediction(dir, id)["model_name_or_path"].clone()),
        ["harness_1", "harness_1"]
    );
}

#[test]
fn predictions_carry_the_model_patch() {
    assert_eq!(
        full().per_task(|dir, id| prediction(dir, id)["model_patch"].clone()),
        ["base\n", "base\n"]
    );
}

#[test]
fn predictions_carry_the_proposed_patches() {
    assert_eq!(
        full().per_task(|dir, id| prediction(dir, id)["proposed_model_patches"].clone()),
        TASKS[..2]
            .iter()
            .map(|id| json!([format!("proposal {id}\n")]))
            .collect::<Vec<_>>()
    );
}

#[test]
fn chat_logs_are_copied_back() {
    assert_eq!(
        full().per_task(|dir, id| text(dir.join(format!("{id}.md")))),
        TASKS[..2]
            .iter()
            .map(|id| format!("chat {id} secret\n"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn extra_chat_logs_are_copied_back() {
    assert_eq!(
        full().per_task(|dir, id| text(dir.join(format!("{id}_1.md")))),
        TASKS[..2]
            .iter()
            .map(|id| format!("more {id}\n"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn tasks_not_asked_for_are_not_run() {
    assert!(!full().dirs[0].join("sympy__sympy-20590.json").exists());
}

#[test]
fn a_second_run_takes_the_next_name() {
    let f = full();
    assert_eq!(f.again(), [f.pred.join("harness_2")]);
}

#[test]
fn a_second_run_writes_its_own_predictions() {
    let f = full();
    assert_eq!(
        read(&f.again()[0].join("astropy__astropy-12907.json"))["model_patch"],
        "base\n"
    );
}

fn spec(id: &str, eval_script: &str, image: &str) -> TestSpec {
    TestSpec {
        instance_id: id.to_owned(),
        eval_script: eval_script.to_owned(),
        image: image.to_owned(),
        platform: platform(),
        user: "root".to_owned(),
        nano_cpus: None,
    }
}

fn evaluate(h: &Evaluator, specs: &[TestSpec], out: &Path, run: &Run) -> Vec<EntryResult> {
    let entries: Vec<Value> = specs
        .iter()
        .map(
            |s| json!({"instance_id": s.instance_id, "problem_statement": "p", "base_commit": "c"}),
        )
        .collect();
    fs::create_dir_all(out).unwrap();
    h.evaluate(&entries, specs, out, run)
}

fn errored(results: &[EntryResult]) -> Vec<String> {
    results
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.instance_id.clone())
        .collect()
}

fn patched(results: &[EntryResult], out: &Path) -> Vec<String> {
    results
        .iter()
        .filter(|r| read(&out.join(format!("{}.json", r.instance_id)))["model_patch"] != "")
        .map(|r| r.instance_id.clone())
        .collect()
}

fn proposed(results: &[EntryResult], out: &Path) -> Vec<String> {
    results
        .iter()
        .filter(|r| {
            read(&out.join(format!("{}.json", r.instance_id)))["proposed_model_patches"]
                != json!([])
        })
        .map(|r| r.instance_id.clone())
        .collect()
}

struct Evaluated {
    _tmp: tempfile::TempDir,
    out: PathBuf,
    results: Vec<EntryResult>,
}

fn steps() -> [TestSpec; 6] {
    let mode = |m: &str| format!("echo {m} > /mode");
    [
        spec("evalfail", "exit 3", IMAGE),
        spec("harnessfail", &mode("fail"), IMAGE),
        spec("norustyhgm", &mode("norustyhgm"), IMAGE),
        spec("nochat", &mode("nochat"), IMAGE),
        spec("nopatch", &mode("nopatch"), IMAGE),
        spec("noimage", "true", "rustyhgm-no-such-image:never"),
    ]
}

fn failing_steps() -> Evaluated {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = evaluator(root, swebench(root, "exit 0", "exit 0"));
    let out = root.join("out");
    let results = evaluate(&h, &steps(), &out, &harness_run(root));
    Evaluated {
        _tmp: tmp,
        out,
        results,
    }
}

#[test]
fn every_entry_gets_a_result() {
    assert_eq!(failing_steps().results.len(), steps().len());
}

#[test]
fn each_failing_step_reports_why() {
    let expected = [
        "Script failed with exit code 3",
        "Script failed with exit code 1",
        "Script failed with exit code 1",
        "Source path /rustyhgm/nochat.md in container not found",
        "Script failed with exit code 1",
        "`docker run -d --name sweb.eval.noimage.",
    ];
    let wrong: Vec<String> = failing_steps()
        .results
        .iter()
        .zip(expected)
        .filter(|(r, text)| !r.error.as_deref().is_some_and(|e| e.starts_with(text)))
        .map(|(r, _)| format!("{r:?}"))
        .collect();
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn each_failing_step_is_an_error() {
    assert_eq!(errored(&failing_steps().results), Vec::<String>::new());
}

#[test]
fn each_failing_step_leaves_an_empty_patch() {
    let e = failing_steps();
    assert_eq!(patched(&e.results, &e.out), Vec::<String>::new());
}

#[test]
fn each_failing_step_leaves_no_proposals() {
    let e = failing_steps();
    assert_eq!(proposed(&e.results, &e.out), Vec::<String>::new());
}

fn without_harness() -> Evaluated {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = evaluator(root, swebench(root, "exit 0", "exit 0"));
    let out = root.join("out");
    let empty = Run {
        harness: root.join("empty"),
        ..harness_run(root)
    };
    let results = evaluate(&h, &[spec("noharness", "true", IMAGE)], &out, &empty);
    Evaluated {
        _tmp: tmp,
        out,
        results,
    }
}

#[test]
fn a_missing_harness_fails_setup() {
    assert_eq!(errored(&without_harness().results), Vec::<String>::new());
}

#[test]
fn a_missing_harness_leaves_an_empty_patch() {
    let e = without_harness();
    assert_eq!(patched(&e.results, &e.out), Vec::<String>::new());
}

#[test]
fn a_missing_harness_leaves_no_proposals() {
    let e = without_harness();
    assert_eq!(proposed(&e.results, &e.out), Vec::<String>::new());
}

fn with_blocked_outputs() -> Evaluated {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = evaluator(root, swebench(root, "exit 0", "exit 0"));
    let out = root.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("done.json"), "{}").unwrap();
    let nowhere = root.join("no/such/dir/file");
    for name in ["unwritable.json", "unwritable_docker.log", "noeval_eval.sh"] {
        crate::dangling(&nowhere, &out.join(name));
    }
    let specs = [
        spec("done", "true", IMAGE),
        spec("unwritable", "true", IMAGE),
        spec("noeval", "true", IMAGE),
    ];
    let results = evaluate(&h, &specs, &out, &harness_run(root));
    Evaluated {
        _tmp: tmp,
        out,
        results,
    }
}

#[test]
fn an_existing_prediction_is_not_an_error() {
    assert_eq!(with_blocked_outputs().results[0].error, None);
}

#[test]
fn an_existing_prediction_is_left_alone() {
    let e = with_blocked_outputs();
    assert_eq!(text(e.out.join("done.json")), "{}");
}

#[test]
fn an_unwritable_prediction_is_reported() {
    let e = with_blocked_outputs();
    assert!(
        e.results[1]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("No such file")),
        "{:?}",
        e.results[1]
    );
}

#[test]
fn an_unwritable_eval_script_is_an_error() {
    assert!(with_blocked_outputs().results[2].error.is_some());
}

#[test]
fn an_unwritable_eval_script_leaves_an_empty_patch() {
    let e = with_blocked_outputs();
    assert_eq!(read(&e.out.join("noeval.json"))["model_patch"], "");
}

fn without_docker() -> Evaluated {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = Evaluator {
        docker: Docker::new("rustyhgm-no-such-docker-program"),
        ..evaluator(root, swebench(root, "exit 0", "exit 0"))
    };
    let out = root.join("out");
    let results = evaluate(&h, &[spec("a", "true", IMAGE)], &out, &harness_run(root));
    Evaluated {
        _tmp: tmp,
        out,
        results,
    }
}

#[test]
fn a_docker_that_cannot_start_fails_every_entry() {
    assert_eq!(errored(&without_docker().results), Vec::<String>::new());
}

#[test]
fn a_docker_that_cannot_start_leaves_empty_patches() {
    let e = without_docker();
    assert_eq!(patched(&e.results, &e.out), Vec::<String>::new());
}

#[test]
fn a_docker_that_cannot_start_leaves_no_proposals() {
    let e = without_docker();
    assert_eq!(proposed(&e.results, &e.out), Vec::<String>::new());
}

fn staged(cached: bool, prepare: &str, specs: &str, pred: &str) -> bool {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    if cached {
        cache_rows(root);
    }
    fs::write(root.join("file"), "").unwrap();
    let blocked = root.join("blocked");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("harness_1"), "").unwrap();
    let python = tempfile::tempdir().unwrap();
    evaluator(root, swebench(python.path(), prepare, specs))
        .run_tasks(
            &["astropy__astropy-12907"],
            &harness_run(root),
            &root.join(pred),
        )
        .is_err()
}

#[test]
fn a_dataset_that_cannot_load_stops_the_evaluator() {
    assert!(staged(false, "exit 0", &spec_line(IMAGE), "pred"));
}

#[test]
fn an_unwritable_prediction_folder_stops_the_evaluator() {
    assert!(staged(true, "exit 0", &spec_line(IMAGE), "file/pred"));
}

#[test]
fn a_failing_image_preparation_stops_the_evaluator() {
    assert!(staged(true, "exit 1", &spec_line(IMAGE), "pred"));
}

#[test]
fn garbled_specs_stop_the_evaluator() {
    assert!(staged(true, "exit 0", "echo nope", "pred"));
}

#[test]
fn a_blocked_run_folder_stops_the_evaluator() {
    assert!(staged(true, "exit 0", &spec_line(IMAGE), "blocked"));
}
