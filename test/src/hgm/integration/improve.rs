use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use hgm::FAILED;
use hgm::improve::{Improver, SelfImprove};
use hgm_agent::llm::{Client, Provider};
use hgm_sandbox::Docker;
use serde_json::{Value, json};

use crate::common::{DIAGNOSIS, node, performance, read_json, reply, serve};

pub const IMAGE: &str = "hgm-fake-improve-agent:latest";

const GIT: &str = r#"#!/bin/sh
mode=$(cat /hgm/mode 2>/dev/null)
case "$*" in
  log) [ "$mode" = nolog ] || printf 'commit 0123abc\nAuthor: user\n' ;;
esac
exit 0
"#;

const CARGO: &str = r#"#!/bin/sh
mode=$(cat /hgm/mode 2>/dev/null)
count=$(( $(cat /tmp/builds 2>/dev/null || echo 0) + 1 ))
echo "$count" > /tmp/builds
[ "$mode" = buildfail ] && { echo build failed; exit 1; }
[ "$mode" = childfail ] && [ "$count" -ge 2 ] && { echo child build failed; exit 1; }
out="target/$(uname -m)-unknown-linux-musl/release"
mkdir -p "$out"
cp /usr/local/lib/fake-agent "$out/hgm-agent"
"#;

const AGENT: &str = r#"#!/bin/sh
mode=$(cat /hgm/mode 2>/dev/null)
while [ $# -gt 0 ]; do
  case "$1" in
    --chat_history_file) chat=$2; shift ;;
    --base_commit) base=$2; shift ;;
    --polyglot) kind=" polyglot" ;;
  esac
  shift
done
[ "$mode" = nochat ] || echo "improved from $base$kind" > "$chat"
if [ "$mode" = nopatch ]; then
  : > /hgm/model_patch.diff
else
  echo "patch from $base" > /hgm/model_patch.diff
fi
"#;

pub fn image() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = dir.path();
    fs::create_dir_all(ctx.join("hgm")).unwrap();
    fs::write(ctx.join("hgm/Cargo.toml"), "").unwrap();
    fs::write(ctx.join("git"), GIT).unwrap();
    fs::write(ctx.join("cargo"), CARGO).unwrap();
    fs::write(ctx.join("agent"), AGENT).unwrap();
    fs::write(
        ctx.join("Dockerfile"),
        "FROM busybox:latest\nCOPY hgm /hgm\nCOPY git cargo /usr/local/bin/\nCOPY agent /usr/local/lib/fake-agent\nRUN chmod 755 /usr/local/bin/git /usr/local/bin/cargo /usr/local/lib/fake-agent\nCMD [\"tail\", \"-f\", \"/dev/null\"]\n",
    )
    .unwrap();
    let status = Command::new("docker")
        .args(["build", "-q", "-t", IMAGE])
        .arg(ctx)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

pub fn mode_patch(mode: &str) -> String {
    format!("--- /dev/null\n+++ b/mode\n@@ -0,0 +1 @@\n+{mode}\n")
}

fn parent(out: &Path, commit: &str, overall: Value, patch: Option<&str>) {
    node(out, commit, None, overall);
    fs::create_dir_all(out.join(commit).join("predictions/run")).unwrap();
    if let Some(patch) = patch {
        fs::write(out.join(commit).join("model_patch.diff"), patch).unwrap();
    }
}

fn improver(out: &Path, polyglot: bool, base_url: String, dataset: Vec<Value>) -> SelfImprove {
    SelfImprove {
        docker: Docker::default(),
        root_dir: out.to_path_buf(),
        output_dir: out.to_path_buf(),
        polyglot,
        dataset: Ok(dataset),
        image_name: IMAGE.into(),
        diagnose: Client {
            provider: Provider::OpenRouter,
            model: "m".into(),
            base_url,
            api_key: None,
        },
        self_improve_llm: "m".into(),
        timeout: 60,
    }
}

fn polyglot_task() -> Vec<Value> {
    vec![json!({
        "instance_id": "python__x",
        "language": "python",
        "problem_statement": "p",
        "reference_tests": "t",
    })]
}

fn run_dir(out: &Path, run_id: &str) -> PathBuf {
    out.join(run_id)
}

fn log_of(out: &Path, run_id: &str) -> String {
    fs::read_to_string(run_dir(out, run_id).join("self_improve.log")).unwrap()
}

fn newest(out: &Path, before: &[PathBuf]) -> String {
    let mut fresh: Vec<PathBuf> = fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("self_improve.log").exists() && !before.contains(p))
        .collect();
    assert_eq!(fresh.len(), 1, "{fresh:?}");
    fresh
        .pop()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn runs(out: &Path) -> Vec<PathBuf> {
    fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("self_improve.log").exists())
        .collect()
}

fn failing_run(improver: &SelfImprove, out: &Path, commit: &str) -> String {
    let before = runs(out);
    assert_eq!(improver.sample_child(commit), FAILED);
    log_of(out, &newest(out, &before))
}

fn a_polyglot_child_is_made_from_the_parent_patch(out: &Path) {
    parent(
        out,
        "good",
        performance(&[], &["python__x"], &[]),
        Some(&mode_patch("ok")),
    );
    let (base, seen) = serve(vec![reply(DIAGNOSIS)]);
    let improver = improver(out, true, base, polyglot_task());
    let child = improver.sample_child("good");
    assert_ne!(child, FAILED, "{}", fs::read_dir(out).unwrap().count());
    let dir = run_dir(out, &child);
    assert_eq!(
        fs::read_to_string(dir.join("model_patch.diff")).unwrap(),
        "patch from 0123abc\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("self_evo.md")).unwrap(),
        "improved from 0123abc polyglot\n"
    );
    assert!(dir.join("hgm-agent").is_file());
    let metadata = read_json(&dir.join("metadata.json"));
    let keys: Vec<&String> = metadata.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "run_id",
            "parent_commit",
            "entry",
            "problem_statement",
            "overall_performance"
        ]
    );
    assert_eq!(metadata["entry"], "python__x");
    assert_eq!(metadata["parent_commit"], "good");
    assert!(
        metadata["problem_statement"]
            .as_str()
            .unwrap()
            .contains("broken")
    );
    assert_eq!(metadata["overall_performance"]["accuracy_score"], 0.0);
    assert_eq!(seen.lock().unwrap().len(), 1);
    let containers = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&containers.stdout).contains(&format!("hgm-container-{child}"))
    );
}

fn broken_containers_fail_the_child(out: &Path) {
    let unused = "http://127.0.0.1:9/v1".to_owned();
    let improver = improver(out, true, unused.clone(), polyglot_task());
    for (commit, patch, expected) in [
        ("nolog", mode_patch("nolog"), "git log printed no commit"),
        (
            "buildfail",
            mode_patch("buildfail"),
            "Error while sampling a child: ",
        ),
        (
            "badpatch",
            "--- a/none\n+++ b/none\n@@ -1 +1 @@\n-x\n+y\n".to_owned(),
            "Error while sampling a child: ",
        ),
    ] {
        parent(
            out,
            commit,
            performance(&[], &["python__x"], &[]),
            Some(&patch),
        );
        let log = failing_run(&improver, out, commit);
        assert!(log.contains(expected), "{commit}: {log}");
    }
    parent(out, "unlisted", performance(&[], &["other"], &[]), None);
    let log = failing_run(&improver, out, "unlisted");
    assert!(
        log.contains("Could not find entry with id other in dataset."),
        "{log}"
    );
    parent(out, "solved", performance(&[], &[], &[]), None);
    let log = failing_run(&improver, out, "solved");
    assert!(
        log.contains("Error choosing entry: Failed to choose an entry"),
        "{log}"
    );
    let log = failing_run(&improver, out, "unknown");
    assert!(log.contains("Error while sampling a child: "), "{log}");
    let (base, _) = serve(vec![reply(DIAGNOSIS)]);
    parent(
        out,
        "nopatch",
        performance(&[], &["python__x"], &[]),
        Some(&mode_patch("nopatch")),
    );
    let log = failing_run(&improver_with(out, true, base), out, "nopatch");
    assert!(
        log.contains("Model patch file is empty or does not exist"),
        "{log}"
    );
    let (base, _) = serve(vec![reply(DIAGNOSIS)]);
    parent(
        out,
        "nochat",
        performance(&[], &["python__x"], &[]),
        Some(&mode_patch("nochat")),
    );
    let log = failing_run(&improver_with(out, true, base), out, "nochat");
    assert!(log.contains("Error copying from container: "), "{log}");
    assert!(log.contains("Error while sampling a child: "), "{log}");
    let (base, _) = serve(vec![reply(DIAGNOSIS)]);
    parent(
        out,
        "childfail",
        performance(&[], &["python__x"], &[]),
        Some(&mode_patch("childfail")),
    );
    let log = failing_run(&improver_with(out, true, base), out, "childfail");
    assert!(log.contains("child build failed"), "{log}");
    assert!(log.contains("Error while sampling a child: "), "{log}");
}

fn improver_with(out: &Path, polyglot: bool, base: String) -> SelfImprove {
    improver(out, polyglot, base, polyglot_task())
}

fn an_undiagnosed_swe_child_fails(out: &Path) {
    parent(out, "swe", performance(&[], &["u"], &[]), None);
    let (base, seen) = serve(vec![reply("no"), reply("no"), reply("no")]);
    let dataset = vec![json!({"instance_id": "u", "test_patch": "t", "problem_statement": "p"})];
    let improver = improver(out, false, base, dataset);
    let before = runs(out);
    assert_eq!(improver.sample_child("swe"), FAILED);
    let run_id = newest(out, &before);
    let log = log_of(out, &run_id);
    assert!(
        log.contains("Failed to diagnose the problem statement. Exiting."),
        "{log}"
    );
    let metadata = read_json(&run_dir(out, &run_id).join("metadata.json"));
    assert_eq!(metadata["problem_statement"], Value::Null);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn self_improvement_runs_the_agent_in_a_container() {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    a_polyglot_child_is_made_from_the_parent_patch(out);
    broken_containers_fail_the_child(out);
    an_undiagnosed_swe_child_fails(out);
}
