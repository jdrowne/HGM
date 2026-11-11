use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use hgm_bench::polyglot::{Harness, Outcome};
use hgm_bench::polyglot_spec::host_arch;
use hgm_bench::swe::Run;
use hgm_sandbox::Docker;
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

const PYTHON: &str = r#"#!/bin/sh
mode=$(cat /testbed/mode 2>/dev/null)
if [ "$1" = -m ]; then
  [ "$mode" = pipfail ] && exit 1
  exit 0
fi
while [ $# -gt 0 ]; do
  case "$1" in
    --chat_history_file) chat=$2; shift ;;
    --language) lang=$2; shift ;;
  esac
  shift
done
id=$(basename "$chat" .md)
[ "$mode" = fail ] && { echo agent failed; exit 1; }
[ "$mode" = nochat ] || echo "chat $id $lang $(pwd)" > "$chat"
echo "more $id" > "/hgm/${id}_1.md"
[ "$mode" = space ] && echo x > "/hgm/${id}_ x.md"
case "$mode" in
  nopatch) ;;
  empty) : > /hgm/model_patch.diff ;;
  *) cat /hgm/applied.txt > /hgm/model_patch.diff 2>/dev/null || echo "patch $id" > /hgm/model_patch.diff ;;
esac
exit 0
"#;

const PYTEST: &str = r#"#!/bin/sh
[ "$(cat /testbed/mode 2>/dev/null)" = unresolved ] && { echo 1 failed; exit 1; }
echo 1 passed
"#;

fn base_image() {
    let tag = format!("pb.base.{}:latest", host_arch());
    let label = Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .Config.Labels \"hgm.fake\"}}",
            &tag,
        ])
        .output()
        .unwrap();
    let found = String::from_utf8_lossy(&label.stdout).trim().to_owned();
    assert!(
        !label.status.success() || found == "polyglot",
        "{tag} is a real polyglot base image; remove or retag it before running these tests"
    );
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
            "FROM busybox:latest\nLABEL hgm.fake=polyglot\nRUN echo {stamp} > /stamp\nCOPY bash /bin/bash\nCOPY git python pytest conda /usr/local/bin/\nRUN chmod 755 /bin/bash /usr/local/bin/git /usr/local/bin/python /usr/local/bin/pytest /usr/local/bin/conda && mkdir -p /opt/miniconda3/bin && echo export CONDA_DEFAULT_ENV=testbed > /opt/miniconda3/bin/activate && touch /root/.bashrc\n"
        ),
    )
    .unwrap();
    let status = Command::new("docker")
        .args(["build", "-q", "-t", &tag])
        .arg(dir.path())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

const MODES: [&str; 10] = [
    "resolved",
    "unresolved",
    "empty",
    "nopatch",
    "gitfail",
    "nochat",
    "fail",
    "pipfail",
    "space",
    "patched",
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
        docker: Docker::default(),
        root: root.to_path_buf(),
        dataset,
        machine: std::env::consts::ARCH.to_owned(),
        llm: "m".into(),
        timeout: 60,
    }
}

fn agent(root: &Path, patches: Vec<PathBuf>) -> Run {
    let dir = root.join("agent");
    for d in ["tools", "utils", "tests", "prompts"] {
        fs::create_dir_all(dir.join(d)).unwrap();
        fs::write(dir.join(d).join("x.py"), "").unwrap();
    }
    for f in [
        "coding_agent_polyglot.py",
        "requirements.txt",
        "pytest.ini",
        "llm.py",
        "llm_withtools.py",
        "LICENSE.txt",
    ] {
        fs::write(dir.join(f), "").unwrap();
    }
    Run {
        model_name_or_path: "agent".into(),
        model_patch_paths: patches,
        init_agent_path: dir,
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

const APPLIED: &str = "diff --git a/applied.txt b/applied.txt\n--- /dev/null\n+++ b/applied.txt\n@@ -0,0 +1 @@\n+applied\ndiff --git a/polyglot/x.txt b/polyglot/x.txt\n--- /dev/null\n+++ b/polyglot/x.txt\n@@ -0,0 +1 @@\n+dropped\n";

fn check_report(report: &Value) {
    assert_eq!(report["total_instances"], 10);
    assert_eq!(report["submitted_instances"], 9);
    assert_eq!(report["resolved_ids"], json!(["python__resolved"]));
    assert_eq!(report["unresolved_ids"], json!(["python__unresolved"]));
    assert_eq!(report["empty_patch_ids"], json!(["python__empty"]));
    assert_eq!(report["error_ids"], json!([]));
    assert_eq!(
        report["incomplete_ids"],
        json!([
            "python__fail",
            "python__gitfail",
            "python__nochat",
            "python__nopatch",
            "python__pipfail",
            "python__space"
        ])
    );
}

fn check_results(out: &Path) {
    let result = |id: &str| read(&out.join(format!("python__{id}.json")));
    let resolved = result("resolved");
    assert_eq!(resolved["model_patch"], "patch python__resolved\n");
    assert_eq!(resolved["model_name_or_path"], "agent_0");
    assert_eq!(resolved["success"], true);
    assert_eq!(resolved["proposed_model_patches"], json!([]));
    let gitfail = result("gitfail");
    assert_eq!(gitfail["eval_result"], "error");
    assert_eq!(gitfail["proposed_model_patches"], json!([]));
    assert_eq!(gitfail["model_patch"], "patch python__gitfail\n");
    assert_eq!(result("nochat")["eval_result"], "incomplete");
    assert_eq!(result("nochat")["proposed_model_patches"], "");
    assert_eq!(result("empty")["eval_result"], "empty_patch");
}

fn check_outputs(out: &Path) {
    let text = |name: &str| fs::read_to_string(out.join(name)).unwrap();
    assert_eq!(
        text("python__resolved.md"),
        "chat python__resolved python /testbed\n"
    );
    assert_eq!(text("python__resolved_1.md"), "more python__resolved\n");
    assert!(text("python__resolved_eval.md").contains("1 passed"));
    assert_eq!(
        text("python__resolved_eval.sh"),
        "#!/bin/bash\nset -uxo pipefail\npytest -rA --tb=long\n"
    );
    let docker_log = text("python__resolved_docker.log");
    assert!(docker_log.contains("Running the eval"), "{docker_log}");
    assert!(docker_log.contains("Stopping container"), "{docker_log}");
    let fail_log = text("python__fail_docker.log");
    assert!(
        fail_log.contains("Error processing entry python__fail: "),
        "{fail_log}"
    );
}

fn run_every_mode(h: &Harness, root: &Path, commit: &Path) {
    let mut tasks: Vec<String> = MODES
        .iter()
        .filter(|m| **m != "patched")
        .map(|m| format!("python__{m}"))
        .collect();
    tasks.push("python__absent".to_owned());
    let pred = root.join("pred");
    let report_file = h
        .harness(&tasks, &agent(root, vec![]), &pred, commit)
        .unwrap();
    assert!(report_file.starts_with(commit));
    let name = report_file
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with("agent "), "{name}");
    let report = read(&report_file);
    check_report(&report);
    check_results(&pred.join("agent_0"));
    check_outputs(&pred.join("agent_0"));
    let again = h
        .harness(&tasks, &agent(root, vec![]), &pred, commit)
        .unwrap();
    assert_eq!(read(&again)["resolved_ids"], json!(["python__resolved"]));
    assert_eq!(read(&again)["incomplete_ids"], report["incomplete_ids"]);
}

fn run_with_a_parent_patch(h: &Harness, root: &Path, commit: &Path) {
    let patch = root.join("parent.diff");
    fs::write(&patch, APPLIED).unwrap();
    let pred = root.join("patched");
    let report = h
        .harness(
            &["python__patched"],
            &agent(root, vec![patch.clone()]),
            &pred,
            commit,
        )
        .unwrap();
    assert_eq!(read(&report)["resolved_ids"], json!(["python__patched"]));
    assert_eq!(
        read(&pred.join("agent_0/python__patched.json"))["model_patch"],
        "applied\n"
    );
    assert!(!fs::read_to_string(&patch).unwrap().contains("polyglot"));
}

fn run_with_unwritable_outputs(h: &Harness, root: &Path) {
    let unwritable = root.join("unwritable");
    fs::create_dir_all(&unwritable).unwrap();
    let nowhere = root.join("no/such/dir/file");
    for name in [
        "python__empty.json",
        "python__resolved_eval.sh",
        "python__unresolved_eval.md",
    ] {
        symlink(&nowhere, unwritable.join(name)).unwrap();
    }
    let entries = [entry("empty"), entry("resolved"), entry("unresolved")];
    let mut outcomes = h.evaluate(&entries, &unwritable, &agent(root, vec![]));
    outcomes.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    assert_eq!(
        outcomes,
        [
            outcome("python__empty", false, "error"),
            outcome("python__resolved", false, "error"),
            outcome("python__unresolved", false, "error"),
        ]
    );
    assert_eq!(
        read(&unwritable.join("python__resolved.json"))["eval_result"],
        "error"
    );
    let lost = h.evaluate(&[entry("ghost")], &root.join("lost"), &agent(root, vec![]));
    assert_eq!(lost, [outcome("python__ghost", false, "incomplete")]);
}

#[test]
fn the_polyglot_harness_builds_images_runs_agents_and_reports() {
    base_image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = harness(root, root_with_tasks(root, &MODES));
    let commit = root.join("commit");
    fs::create_dir_all(&commit).unwrap();
    run_every_mode(&h, root, &commit);
    base_image();
    run_with_a_parent_patch(&h, root, &commit);
    run_with_unwritable_outputs(&h, root);
}

#[test]
fn harness_setup_failures_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let dataset = root_with_tasks(root, &["resolved"]);
    let run = agent(root, vec![]);
    let commit = root.join("commit");
    let tasks = ["python__resolved"];
    let gone = Harness {
        docker: Docker::new("hgm-no-such-docker-program"),
        ..harness(root, dataset.clone())
    };
    assert!(
        gone.harness(&tasks, &run, &root.join("pred"), &commit)
            .is_err()
    );
    let missing_patch = agent(root, vec![root.join("no.diff")]);
    assert!(
        gone.harness(&tasks, &missing_patch, &root.join("pred"), &commit)
            .is_err()
    );
    let no_dataset = harness(root, root.join("none.json"));
    let e = no_dataset
        .harness(&tasks, &run, &root.join("pred"), &commit)
        .unwrap_err();
    assert!(e.starts_with("Dataset file not found: "), "{e}");
    let junk = root.join("junk.json");
    fs::write(&junk, "[").unwrap();
    assert!(
        harness(root, junk)
            .harness(&tasks, &run, &root.join("pred"), &commit)
            .is_err()
    );
    let file = root.join("file");
    fs::write(&file, "").unwrap();
    assert!(
        gone.harness(&tasks, &run, &file.join("pred"), &commit)
            .is_err()
    );
    let empty = Harness {
        docker: Docker::new("hgm-no-such-docker-program"),
        ..harness(root, dataset.clone())
    };
    let blocked = root.join("blocked");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("agent_0"), "").unwrap();
    assert!(empty.harness(&["none"], &run, &blocked, &commit).is_err());
    let nowhere = empty.harness(&["none"], &run, &root.join("pred"), &root.join("no/such"));
    assert!(nowhere.is_err());
    let report = empty
        .harness(&["none"], &run, &root.join("pred"), root)
        .unwrap();
    let report = read(&report);
    assert_eq!(report["total_instances"], 1);
    assert_eq!(report["submitted_instances"], 0);
    let outcomes = empty.evaluate(&[entry("resolved")], &root.join("out"), &run);
    assert_eq!(outcomes, [outcome("python__resolved", false, "incomplete")]);
}
