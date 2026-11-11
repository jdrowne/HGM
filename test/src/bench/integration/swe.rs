use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use hgm_bench::dataset::{Hub, SWE_BENCH};
use hgm_bench::swe::{EntryResult, Harness, Run};
use hgm_bench::swebench::{Swebench, TestSpec};
use hgm_sandbox::Docker;
use serde_json::{Value, json};

const IMAGE: &str = "hgm-bench-fake-swe:latest";

const AGENT: &str = r#"#!/bin/sh
mode=$(cat /mode 2>/dev/null)
while [ $# -gt 0 ]; do
  case "$1" in
    --instance_id) id=$2; shift ;;
    --chat_history_file) chat=$2; shift ;;
  esac
  shift
done
case "$mode" in
  fail) echo agent failed; exit 1 ;;
  nohgm) rm -rf /hgm; exit 0 ;;
esac
[ "$mode" = nochat ] || echo "chat $id" > "$chat"
echo "more $id" > "/hgm/${id}_1.md"
[ "$mode" = nopatch ] || echo base > /hgm/model_patch.diff
echo "proposal $id" > /hgm/model_patch_1.diff
"#;

fn platform() -> String {
    let out = Command::new("docker")
        .args(["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
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
    assert!(status.success());
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
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

fn harness(root: &Path, swebench: Swebench) -> Harness {
    Harness {
        docker: Docker::default(),
        swebench,
        hub: Hub {
            endpoint: refused(),
            cache_dir: root.join("cache"),
        },
        llm: "m".into(),
        timeout: 60,
    }
}

fn refused() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn agent_run(root: &Path) -> Run {
    let dir = root.join("agent");
    fs::create_dir_all(&dir).unwrap();
    Run {
        model_name_or_path: "agent".into(),
        agent: PathBuf::from(script(
            &dir,
            "hgm-agent",
            AGENT.trim_start_matches("#!/bin/sh\n"),
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

#[test]
fn a_full_run_writes_predictions_and_copies_logs_back() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    cache_rows(root);
    let h = harness(root, swebench(root, "exit 0", &spec_line(IMAGE)));
    let run = agent_run(root);
    let pred = root.join("pred");
    let tasks = ["astropy__astropy-12907", "django__django-11099", "absent"];
    let dirs = h.harness(&tasks, &run, &pred).unwrap();
    assert_eq!(dirs, [pred.join("agent_1")]);
    for id in &tasks[..2] {
        let p = read(&dirs[0].join(format!("{id}.json")));
        assert_eq!(p["model_name_or_path"], "agent_1");
        assert_eq!(p["model_patch"], "base\n");
        assert_eq!(
            p["proposed_model_patches"],
            json!([format!("proposal {id}\n")])
        );
        assert_eq!(
            fs::read_to_string(dirs[0].join(format!("{id}.md"))).unwrap(),
            format!("chat {id}\n")
        );
        assert_eq!(
            fs::read_to_string(dirs[0].join(format!("{id}_1.md"))).unwrap(),
            format!("more {id}\n")
        );
    }
    assert!(!dirs[0].join("sympy__sympy-20590.json").exists());
    let again = h.harness(&tasks[..1], &agent_run(root), &pred).unwrap();
    assert_eq!(again, [pred.join("agent_2")]);
    assert_eq!(
        read(&again[0].join("astropy__astropy-12907.json"))["model_patch"],
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

fn evaluate(h: &Harness, specs: &[TestSpec], out: &Path, run: &Run) -> Vec<EntryResult> {
    let entries: Vec<Value> = specs
        .iter()
        .map(
            |s| json!({"instance_id": s.instance_id, "problem_statement": "p", "base_commit": "c"}),
        )
        .collect();
    fs::create_dir_all(out).unwrap();
    h.evaluate(&entries, specs, out, run)
}

fn assert_failed_with_an_empty_patch(results: &[EntryResult], out: &Path) {
    for r in results {
        assert!(r.error.is_some(), "{r:?}");
        let p = read(&out.join(format!("{}.json", r.instance_id)));
        assert_eq!(p["model_patch"], "", "{r:?}");
        assert_eq!(p["proposed_model_patches"], json!([]));
    }
}

#[test]
fn each_failing_step_leaves_an_empty_prediction() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = harness(root, swebench(root, "exit 0", "exit 0"));
    let mode = |m: &str| format!("echo {m} > /mode");
    let specs = [
        spec("evalfail", "exit 3", IMAGE),
        spec("agentfail", &mode("fail"), IMAGE),
        spec("nohgm", &mode("nohgm"), IMAGE),
        spec("nochat", &mode("nochat"), IMAGE),
        spec("nopatch", &mode("nopatch"), IMAGE),
        spec("noimage", "true", "hgm-no-such-image:never"),
    ];
    let out = root.join("out");
    let results = evaluate(&h, &specs, &out, &agent_run(root));
    assert_eq!(results.len(), specs.len());
    let expected = [
        "Script failed with exit code 3",
        "Script failed with exit code 1",
        "Script failed with exit code 1",
        "Source path /hgm/nochat.md in container not found",
        "Script failed with exit code 1",
        "`docker run -d --name sweb.eval.noimage.",
    ];
    for (r, text) in results.iter().zip(expected) {
        assert!(
            r.error.as_deref().is_some_and(|e| e.starts_with(text)),
            "{r:?}"
        );
    }
    assert_failed_with_an_empty_patch(&results, &out);
}

#[test]
fn a_missing_agent_fails_setup() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = harness(root, swebench(root, "exit 0", "exit 0"));
    let out = root.join("out");
    let empty = Run {
        agent: root.join("empty"),
        ..agent_run(root)
    };
    let results = evaluate(&h, &[spec("noagent", "true", IMAGE)], &out, &empty);
    assert_failed_with_an_empty_patch(&results, &out);
}

#[test]
fn existing_predictions_are_skipped_and_unwritable_outputs_are_reported() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = harness(root, swebench(root, "exit 0", "exit 0"));
    let out = root.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("done.json"), "{}").unwrap();
    let nowhere = root.join("no/such/dir/file");
    for name in ["unwritable.json", "unwritable_docker.log", "noeval_eval.sh"] {
        symlink(&nowhere, out.join(name)).unwrap();
    }
    let specs = [
        spec("done", "true", IMAGE),
        spec("unwritable", "true", IMAGE),
        spec("noeval", "true", IMAGE),
    ];
    let results = evaluate(&h, &specs, &out, &agent_run(root));
    assert_eq!(results[0].error, None);
    assert_eq!(fs::read_to_string(out.join("done.json")).unwrap(), "{}");
    assert!(
        results[1]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("No such file")),
        "{:?}",
        results[1]
    );
    assert!(results[2].error.is_some());
    assert_eq!(read(&out.join("noeval.json"))["model_patch"], "");
}

#[test]
fn a_docker_that_cannot_start_fails_every_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let h = Harness {
        docker: Docker::new("hgm-no-such-docker-program"),
        ..harness(root, swebench(root, "exit 0", "exit 0"))
    };
    let out = root.join("out");
    let results = evaluate(&h, &[spec("a", "true", IMAGE)], &out, &agent_run(root));
    assert_failed_with_an_empty_patch(&results, &out);
}

#[test]
fn the_harness_stops_at_the_first_failing_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let tasks = ["astropy__astropy-12907"];
    let r = agent_run(root);
    let ok = swebench(root, "exit 0", &spec_line(IMAGE));
    assert!(
        harness(root, ok.clone())
            .harness(&tasks, &r, &root.join("pred"))
            .is_err()
    );
    cache_rows(root);
    fs::write(root.join("file"), "").unwrap();
    assert!(
        harness(root, ok.clone())
            .harness(&tasks, &r, &root.join("file/pred"))
            .is_err()
    );
    let failing = tempfile::tempdir().unwrap();
    let prepare = swebench(failing.path(), "exit 1", &spec_line(IMAGE));
    assert!(
        harness(root, prepare)
            .harness(&tasks, &r, &root.join("pred"))
            .is_err()
    );
    let garbled = tempfile::tempdir().unwrap();
    let specs = swebench(garbled.path(), "exit 0", "echo nope");
    assert!(
        harness(root, specs)
            .harness(&tasks, &r, &root.join("pred"))
            .is_err()
    );
    let pred = root.join("blocked");
    fs::create_dir_all(&pred).unwrap();
    fs::write(pred.join("agent_1"), "").unwrap();
    assert!(harness(root, ok).harness(&tasks, &r, &pred).is_err());
}
