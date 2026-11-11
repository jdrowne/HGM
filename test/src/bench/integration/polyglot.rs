use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use rustyhgm_bench::polyglot::{Harness, Outcome};
use rustyhgm_bench::polyglot_spec::host_arch;
use rustyhgm_bench::swe::Run;
use rustyhgm_sandbox::Docker;
use serde_json::{Value, json};

const BASH: &str = "#!/bin/sh\nexec /bin/sh \"$@\"\n";

const GIT: &str = r#"#!/bin/sh
[ "$1" = -C ] && { cd "$2"; shift 2; }
case "$1" in
  clone) mkdir -p "$3" && cp -a ./. "$3"/ ;;
  stash) [ "$(cat /testbed/mode 2>/dev/null)" = gitfail ] && { echo git failed; exit 1; } ;;
esac
exit 0
"#;

const PYTHON: &str = "#!/bin/sh\nexit 0\n";

const AGENT: &str = r#"#!/bin/sh
mode=$(cat /testbed/mode 2>/dev/null)
while [ $# -gt 0 ]; do
  case "$1" in
    --chat_history_file) chat=$2; shift ;;
    --api_key_file) key=$2; shift ;;
    --language) lang=$2; shift ;;
    --polyglot) [ "$2" = true ] && kind=polyglot; shift ;;
  esac
  shift
done
id=$(basename "$chat" .md)
[ "$mode" = fail ] && { echo agent failed; exit 1; }
[ "$mode" = nochat ] || echo "chat $id $kind $lang $(pwd) $(cat "$key")" > "$chat"
echo "more $id" > "/rustyhgm/${id}_1.md"
[ "$mode" = space ] && echo x > "/rustyhgm/${id}_ x.md"
case "$mode" in
  nopatch) ;;
  empty) : > /rustyhgm/model_patch.diff ;;
  *) echo "patch $id" > /rustyhgm/model_patch.diff ;;
esac
exit 0
"#;

const FAKE: &str = "rustyhgm-fake";

const PYTEST: &str = r#"#!/bin/sh
[ "$(cat /testbed/mode 2>/dev/null)" = unresolved ] && { echo 1 failed; exit 1; }
echo 1 passed
"#;

fn base_image() {
    let tag = format!("{FAKE}.base.{}:latest", host_arch());
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in [
        ("bash", BASH),
        ("git", GIT),
        ("python", PYTHON),
        ("pytest", PYTEST),
        ("conda", "#!/bin/sh\nexit 0\n"),
    ] {
        fs::write(dir.path().join(name), body).unwrap();
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    fs::write(
        dir.path().join("Dockerfile"),
        format!(
            "FROM busybox:latest\nLABEL rustyhgm.fake=polyglot\nRUN echo {stamp} > /stamp\nCOPY bash /bin/bash\nCOPY git python pytest conda /usr/local/bin/\nRUN chmod 755 /bin/bash /usr/local/bin/git /usr/local/bin/python /usr/local/bin/pytest /usr/local/bin/conda && mkdir -p /opt/miniconda3/bin && echo export CONDA_DEFAULT_ENV=testbed > /opt/miniconda3/bin/activate && touch /root/.bashrc\n"
        ),
    )
    .unwrap();
    let status = Command::new("docker")
        .args(["build", "-q", "-t", &tag])
        .arg(dir.path())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    if !status.success() {
        panic!("building {tag} failed");
    }
}

const MODES: [&str; 8] = [
    "resolved",
    "unresolved",
    "empty",
    "nopatch",
    "gitfail",
    "nochat",
    "fail",
    "space",
];

fn entry(mode: &str) -> Value {
    json!({
        "instance_id": format!("python__{mode}"),
        "language": "python",
        "repo": format!("polyglot/polyglot-benchmark/python/exercises/practice/{mode}"),
        "base_commit": "c",
        "test_commit": "t",
        "problem_statement": "p",
        "test_patch": "",
        "files": {"solution": [format!("{mode}.py")]},
    })
}

fn root_with_tasks(root: &Path, modes: &[&str]) -> PathBuf {
    for mode in modes {
        let task = root.join(format!(
            "polyglot/polyglot-benchmark/python/exercises/practice/{mode}"
        ));
        fs::create_dir_all(&task).unwrap();
        fs::write(task.join("mode"), mode).unwrap();
        fs::write(task.join(format!("{mode}.py")), "").unwrap();
    }
    let dataset = root.join("polyglot/meta.json");
    let entries: Vec<Value> = modes.iter().map(|m| entry(m)).collect();
    fs::write(&dataset, serde_json::to_string(&entries).unwrap()).unwrap();
    dataset
}

fn harness(root: &Path, dataset: PathBuf) -> Harness {
    Harness {
        docker: Docker::new("docker"),
        root: root.to_path_buf(),
        dataset,
        machine: std::env::consts::ARCH.to_owned(),
        namespace: FAKE.into(),
        llm: "m".into(),
        service: crate::service(root),
        timeout: 60,
    }
}

fn agent(root: &Path) -> Run {
    let path = root.join("agent/rustyhgm-coding-agent");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    crate::executable(&path, AGENT);
    Run {
        model_name_or_path: "agent".into(),
        agent: path,
        max_workers: 10,
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn outcome(id: &str, success: bool, eval_result: &str) -> Outcome {
    Outcome {
        instance_id: id.to_owned(),
        success,
        eval_result: eval_result.to_owned(),
    }
}

fn outcomes_json(outcomes: &[Outcome]) -> Value {
    json!(
        outcomes
            .iter()
            .map(|o| json!([o.instance_id, o.success, o.eval_result]))
            .collect::<Vec<_>>()
    )
}

fn expected(outcomes: &[Outcome]) -> Value {
    outcomes_json(outcomes)
}

fn scenario(root: &Path) {
    base_image();
    let h = harness(root, root_with_tasks(root, &MODES));
    let commit = root.join("commit");
    fs::create_dir_all(&commit).unwrap();
    let mut tasks: Vec<String> = MODES.iter().map(|m| format!("python__{m}")).collect();
    tasks.push("python__absent".to_owned());
    let pred = root.join("pred");
    let report = h.harness(&tasks, &agent(root), &pred, &commit).unwrap();
    let again = h.harness(&tasks, &agent(root), &pred, &commit).unwrap();
    let run = Run {
        agent: root.join("no-agent"),
        ..agent(root)
    };
    let noagent = h.evaluate(&[entry("resolved")], &root.join("noagent"), &run);
    let unwritable = root.join("unwritable");
    fs::create_dir_all(&unwritable).unwrap();
    let nowhere = root.join("no/such/dir/file");
    for name in [
        "python__empty.json",
        "python__resolved_eval.sh",
        "python__unresolved_eval.md",
    ] {
        crate::dangling(&nowhere, &unwritable.join(name));
    }
    let entries = [entry("empty"), entry("resolved"), entry("unresolved")];
    let mut blocked = h.evaluate(&entries, &unwritable, &agent(root));
    blocked.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    let lost = h.evaluate(&[entry("ghost")], &root.join("lost"), &agent(root));
    let seen = json!({
        "report": report,
        "again": again,
        "noagent": outcomes_json(&noagent),
        "blocked": outcomes_json(&blocked),
        "lost": outcomes_json(&lost),
    });
    fs::write(root.join("seen.json"), seen.to_string()).unwrap();
}

struct Shared {
    root: PathBuf,
    seen: Value,
}

impl Shared {
    fn path(&self, key: &str) -> PathBuf {
        PathBuf::from(self.seen[key].as_str().unwrap())
    }

    fn report(&self) -> Value {
        read(&self.path("report"))
    }

    fn result(&self, mode: &str) -> Value {
        read(&self.out().join(format!("python__{mode}.json")))
    }

    fn out(&self) -> PathBuf {
        self.root.join("pred/agent_0")
    }

    fn text(&self, name: &str) -> String {
        fs::read_to_string(self.out().join(name)).unwrap()
    }
}

fn shared() -> Shared {
    let root = crate::once("polyglot", scenario);
    let seen = read(&root.join("seen.json"));
    Shared { root, seen }
}

#[test]
fn the_report_lands_in_the_commit_folder() {
    let s = shared();
    assert!(s.path("report").starts_with(s.root.join("commit")));
}

#[test]
fn the_report_is_named_for_the_agent() {
    let name = shared()
        .path("report")
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with("agent "), "{name}");
}

#[test]
fn the_report_counts_every_instance() {
    assert_eq!(shared().report()["total_instances"], 8);
}

#[test]
fn the_report_counts_every_submission() {
    assert_eq!(shared().report()["submitted_instances"], 8);
}

#[test]
fn the_report_lists_the_resolved() {
    assert_eq!(
        shared().report()["resolved_ids"],
        json!(["python__resolved"])
    );
}

#[test]
fn the_report_lists_the_unresolved() {
    assert_eq!(
        shared().report()["unresolved_ids"],
        json!(["python__unresolved"])
    );
}

#[test]
fn the_report_lists_the_empty_patches() {
    assert_eq!(
        shared().report()["empty_patch_ids"],
        json!(["python__empty"])
    );
}

#[test]
fn the_report_lists_no_errors() {
    assert_eq!(shared().report()["error_ids"], json!([]));
}

#[test]
fn the_report_lists_the_incomplete() {
    assert_eq!(
        shared().report()["incomplete_ids"],
        json!([
            "python__fail",
            "python__gitfail",
            "python__nochat",
            "python__nopatch",
            "python__space"
        ])
    );
}

#[test]
fn a_resolved_result_keeps_its_patch() {
    assert_eq!(
        shared().result("resolved")["model_patch"],
        "patch python__resolved\n"
    );
}

#[test]
fn a_resolved_result_names_the_run() {
    assert_eq!(shared().result("resolved")["model_name_or_path"], "agent_0");
}

#[test]
fn a_resolved_result_succeeds() {
    assert_eq!(shared().result("resolved")["success"], true);
}

#[test]
fn a_resolved_result_has_no_proposals() {
    assert_eq!(
        shared().result("resolved")["proposed_model_patches"],
        json!([])
    );
}

#[test]
fn a_failing_git_is_an_eval_error() {
    assert_eq!(shared().result("gitfail")["eval_result"], "error");
}

#[test]
fn a_failing_git_has_no_proposals() {
    assert_eq!(
        shared().result("gitfail")["proposed_model_patches"],
        json!([])
    );
}

#[test]
fn a_failing_git_keeps_the_patch() {
    assert_eq!(
        shared().result("gitfail")["model_patch"],
        "patch python__gitfail\n"
    );
}

#[test]
fn a_missing_chat_is_incomplete() {
    assert_eq!(shared().result("nochat")["eval_result"], "incomplete");
}

#[test]
fn a_missing_chat_has_blank_proposals() {
    assert_eq!(shared().result("nochat")["proposed_model_patches"], "");
}

#[test]
fn an_empty_patch_is_reported() {
    assert_eq!(shared().result("empty")["eval_result"], "empty_patch");
}

#[test]
fn the_chat_log_is_copied_back() {
    assert_eq!(
        shared().text("python__resolved.md"),
        "chat python__resolved polyglot python /testbed secret\n"
    );
}

#[test]
fn extra_chat_logs_are_copied_back() {
    assert_eq!(
        shared().text("python__resolved_1.md"),
        "more python__resolved\n"
    );
}

#[test]
fn the_eval_output_is_kept() {
    assert!(
        shared()
            .text("python__resolved_eval.md")
            .contains("1 passed")
    );
}

#[test]
fn the_eval_script_is_kept() {
    assert_eq!(
        shared().text("python__resolved_eval.sh"),
        "#!/bin/bash\nset -uxo pipefail\npytest -rA --tb=long\n"
    );
}

#[test]
fn the_docker_log_records_the_eval() {
    let log = shared().text("python__resolved_docker.log");
    assert!(log.contains("Running the eval"), "{log}");
}

#[test]
fn the_docker_log_records_the_stop() {
    let log = shared().text("python__resolved_docker.log");
    assert!(log.contains("Stopping container"), "{log}");
}

#[test]
fn a_failing_agent_is_logged() {
    let log = shared().text("python__fail_docker.log");
    assert!(
        log.contains("Error processing entry python__fail: "),
        "{log}"
    );
}

fn again() -> Value {
    read(&shared().path("again"))
}

#[test]
fn a_second_run_resolves_the_same() {
    assert_eq!(again()["resolved_ids"], json!(["python__resolved"]));
}

#[test]
fn a_second_run_leaves_the_same_incomplete() {
    assert_eq!(
        again()["incomplete_ids"],
        shared().report()["incomplete_ids"]
    );
}

#[test]
fn a_run_without_an_agent_is_incomplete() {
    assert_eq!(
        shared().seen["noagent"],
        expected(&[outcome("python__resolved", false, "incomplete")])
    );
}

#[test]
fn unwritable_outputs_are_errors() {
    assert_eq!(
        shared().seen["blocked"],
        expected(&[
            outcome("python__empty", false, "error"),
            outcome("python__resolved", false, "error"),
            outcome("python__unresolved", false, "error"),
        ])
    );
}

#[test]
fn an_unwritable_eval_script_is_recorded_as_an_error() {
    assert_eq!(
        read(&shared().root.join("unwritable/python__resolved.json"))["eval_result"],
        "error"
    );
}

#[test]
fn an_entry_without_a_task_folder_is_incomplete() {
    assert_eq!(
        shared().seen["lost"],
        expected(&[outcome("python__ghost", false, "incomplete")])
    );
}

struct Setup {
    tmp: tempfile::TempDir,
    dataset: PathBuf,
    run: Run,
}

impl Setup {
    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn gone(&self) -> Harness {
        Harness {
            docker: Docker::new("rustyhgm-no-such-docker-program"),
            ..harness(self.root(), self.dataset.clone())
        }
    }

    fn commit(&self) -> PathBuf {
        self.root().join("commit")
    }

    fn pred(&self) -> PathBuf {
        self.root().join("pred")
    }
}

const TASKS: [&str; 1] = ["python__resolved"];

fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let dataset = root_with_tasks(tmp.path(), &["resolved"]);
    let run = agent(tmp.path());
    Setup { tmp, dataset, run }
}

#[test]
fn a_missing_docker_fails_the_harness() {
    let s = setup();
    assert!(
        s.gone()
            .harness(&TASKS, &s.run, &s.pred(), &s.commit())
            .is_err()
    );
}

#[test]
fn a_missing_dataset_is_named() {
    let s = setup();
    let e = harness(s.root(), s.root().join("none.json"))
        .harness(&TASKS, &s.run, &s.pred(), &s.commit())
        .unwrap_err();
    assert!(e.starts_with("Dataset file not found: "), "{e}");
}

#[test]
fn a_junk_dataset_fails_the_harness() {
    let s = setup();
    let junk = s.root().join("junk.json");
    fs::write(&junk, "[").unwrap();
    assert!(
        harness(s.root(), junk)
            .harness(&TASKS, &s.run, &s.pred(), &s.commit())
            .is_err()
    );
}

#[test]
fn a_prediction_folder_under_a_file_fails_the_harness() {
    let s = setup();
    let file = s.root().join("file");
    fs::write(&file, "").unwrap();
    assert!(
        s.gone()
            .harness(&TASKS, &s.run, &file.join("pred"), &s.commit())
            .is_err()
    );
}

#[test]
fn a_blocked_run_folder_fails_the_harness() {
    let s = setup();
    let blocked = s.root().join("blocked");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("agent_0"), "").unwrap();
    assert!(
        s.gone()
            .harness(&["none"], &s.run, &blocked, &s.commit())
            .is_err()
    );
}

#[test]
fn a_missing_commit_folder_fails_the_harness() {
    let s = setup();
    assert!(
        s.gone()
            .harness(&["none"], &s.run, &s.pred(), &s.root().join("no/such"))
            .is_err()
    );
}

fn unknown_task_report() -> Value {
    let s = setup();
    read(
        &s.gone()
            .harness(&["none"], &s.run, &s.pred(), s.root())
            .unwrap(),
    )
}

#[test]
fn an_unknown_task_still_counts_as_an_instance() {
    assert_eq!(unknown_task_report()["total_instances"], 1);
}

#[test]
fn an_unknown_task_is_not_submitted() {
    assert_eq!(unknown_task_report()["submitted_instances"], 0);
}

#[test]
fn an_entry_without_docker_is_incomplete() {
    let s = setup();
    let outcomes = s
        .gone()
        .evaluate(&[entry("resolved")], &s.root().join("out"), &s.run);
    assert_eq!(outcomes, [outcome("python__resolved", false, "incomplete")]);
}
