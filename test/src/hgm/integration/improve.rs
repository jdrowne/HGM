use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use hgm::FAILED;
use hgm::improve::{Improver, SelfImprove};
use hgm_agent::llm::{Client, Provider, Service};
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
    --api_key_file) key=$2; shift ;;
    --polyglot) [ "$2" = true ] && kind=" polyglot"; shift ;;
  esac
  shift
done
[ "$mode" = nochat ] || echo "improved from $base$kind with $(cat "$key")" > "$chat"
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
    if !status.success() {
        panic!("building {IMAGE} failed");
    }
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

fn key_file(out: &Path) -> PathBuf {
    let key = out.join("key");
    fs::write(&key, "secret\n").unwrap();
    key
}

fn improver(out: &Path, polyglot: bool, base_url: String, dataset: Vec<Value>) -> SelfImprove {
    SelfImprove {
        docker: Docker::new("docker"),
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
        self_improve_service: Service {
            provider: Provider::Vllm,
            base_url: "http://h/v1".into(),
            api_key_file: key_file(out),
        },
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

fn runs(out: &Path) -> Vec<PathBuf> {
    fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("self_improve.log").exists())
        .collect()
}

struct Attempt {
    tmp: tempfile::TempDir,
    child: String,
    run: Option<PathBuf>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Attempt {
    fn log(&self) -> String {
        self.run
            .as_ref()
            .map(|run| fs::read_to_string(run.join("self_improve.log")).unwrap())
            .unwrap_or_default()
    }

    fn child_dir(&self) -> PathBuf {
        self.tmp.path().join(&self.child)
    }

    fn requests(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

struct Case<'a> {
    commit: &'a str,
    overall: Option<Value>,
    patch: Option<String>,
    polyglot: bool,
    replies: Vec<(u16, String)>,
    dataset: Vec<Value>,
}

impl Case<'_> {
    fn polyglot(commit: &str, patch: Option<String>) -> Case<'_> {
        Case {
            commit,
            overall: Some(performance(&[], &["python__x"], &[])),
            patch,
            polyglot: true,
            replies: Vec::new(),
            dataset: polyglot_task(),
        }
    }

    fn diagnosed(self) -> Self {
        Case {
            replies: vec![reply(DIAGNOSIS)],
            ..self
        }
    }
}

fn attempt(case: Case<'_>) -> Attempt {
    image();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path();
    if let Some(overall) = case.overall {
        parent(out, case.commit, overall, case.patch.as_deref());
    }
    let (base, seen) = serve(case.replies);
    let improver = improver(out, case.polyglot, base, case.dataset);
    let before = runs(out);
    let child = improver.sample_child(case.commit);
    let fresh: Vec<PathBuf> = runs(out)
        .into_iter()
        .filter(|p| !before.contains(p))
        .collect();
    let run = match fresh.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    };
    Attempt {
        tmp,
        child,
        run,
        seen,
    }
}

fn polyglot_child() -> Attempt {
    attempt(Case::polyglot("good", Some(mode_patch("ok"))).diagnosed())
}

fn child_text(name: &str) -> String {
    fs::read_to_string(polyglot_child().child_dir().join(name)).unwrap()
}

fn child_metadata() -> Value {
    read_json(&polyglot_child().child_dir().join("metadata.json"))
}

#[test]
fn a_polyglot_child_is_made() {
    let a = polyglot_child();
    assert_ne!(a.child, FAILED, "{}", a.log());
}

#[test]
fn a_polyglot_child_keeps_the_agent_patch() {
    assert_eq!(child_text("model_patch.diff"), "patch from 0123abc\n");
}

#[test]
fn a_polyglot_child_keeps_the_agent_chat() {
    assert_eq!(
        child_text("self_evo.md"),
        "improved from 0123abc polyglot with secret\n"
    );
}

#[test]
fn a_polyglot_child_keeps_its_built_agent() {
    assert!(polyglot_child().child_dir().join("hgm-agent").is_file());
}

#[test]
fn a_polyglot_child_records_its_metadata_in_python_order() {
    let metadata = child_metadata();
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
}

#[test]
fn a_polyglot_child_records_its_entry() {
    assert_eq!(child_metadata()["entry"], "python__x");
}

#[test]
fn a_polyglot_child_records_its_parent() {
    assert_eq!(child_metadata()["parent_commit"], "good");
}

#[test]
fn a_polyglot_child_records_the_diagnosis() {
    assert!(
        child_metadata()["problem_statement"]
            .as_str()
            .unwrap()
            .contains("broken")
    );
}

#[test]
fn a_polyglot_child_starts_unscored() {
    assert_eq!(
        child_metadata()["overall_performance"]["accuracy_score"],
        0.0
    );
}

#[test]
fn a_polyglot_child_asks_for_one_diagnosis() {
    assert_eq!(polyglot_child().requests(), 1);
}

#[test]
fn a_polyglot_child_removes_its_container() {
    let a = polyglot_child();
    let containers = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&containers.stdout)
            .contains(&format!("hgm-container-{}", a.child))
    );
}

fn no_log() -> Attempt {
    attempt(Case::polyglot("nolog", Some(mode_patch("nolog"))))
}

fn build_failure() -> Attempt {
    attempt(Case::polyglot("buildfail", Some(mode_patch("buildfail"))))
}

fn bad_patch() -> Attempt {
    attempt(Case::polyglot(
        "badpatch",
        Some("--- a/none\n+++ b/none\n@@ -1 +1 @@\n-x\n+y\n".to_owned()),
    ))
}

fn unlisted() -> Attempt {
    attempt(Case {
        overall: Some(performance(&[], &["other"], &[])),
        ..Case::polyglot("unlisted", None)
    })
}

fn solved() -> Attempt {
    attempt(Case {
        overall: Some(performance(&[], &[], &[])),
        ..Case::polyglot("solved", None)
    })
}

fn unknown() -> Attempt {
    attempt(Case {
        overall: None,
        ..Case::polyglot("unknown", None)
    })
}

fn no_patch() -> Attempt {
    attempt(Case::polyglot("nopatch", Some(mode_patch("nopatch"))).diagnosed())
}

fn no_chat() -> Attempt {
    attempt(Case::polyglot("nochat", Some(mode_patch("nochat"))).diagnosed())
}

fn child_failure() -> Attempt {
    attempt(Case::polyglot("childfail", Some(mode_patch("childfail"))).diagnosed())
}

fn logged(a: &Attempt, text: &str) -> bool {
    a.log().contains(text)
}

#[test]
fn a_container_without_a_commit_fails_the_child() {
    assert_eq!(no_log().child, FAILED);
}

#[test]
fn a_container_without_a_commit_says_so() {
    assert!(logged(&no_log(), "git log printed no commit"));
}

#[test]
fn a_parent_that_does_not_build_fails_the_child() {
    assert_eq!(build_failure().child, FAILED);
}

#[test]
fn a_parent_that_does_not_build_is_logged() {
    assert!(logged(&build_failure(), "Error while sampling a child: "));
}

#[test]
fn a_parent_patch_that_does_not_apply_fails_the_child() {
    assert_eq!(bad_patch().child, FAILED);
}

#[test]
fn a_parent_patch_that_does_not_apply_is_logged() {
    assert!(logged(&bad_patch(), "Error while sampling a child: "));
}

#[test]
fn an_entry_missing_from_the_dataset_fails_the_child() {
    assert_eq!(unlisted().child, FAILED);
}

#[test]
fn an_entry_missing_from_the_dataset_is_named() {
    assert!(logged(
        &unlisted(),
        "Could not find entry with id other in dataset."
    ));
}

#[test]
fn a_parent_with_nothing_left_to_solve_fails_the_child() {
    assert_eq!(solved().child, FAILED);
}

#[test]
fn a_parent_with_nothing_left_to_solve_says_so() {
    assert!(logged(
        &solved(),
        "Error choosing entry: Failed to choose an entry"
    ));
}

#[test]
fn an_unknown_parent_fails_the_child() {
    assert_eq!(unknown().child, FAILED);
}

#[test]
fn an_unknown_parent_is_logged() {
    assert!(logged(&unknown(), "Error while sampling a child: "));
}

#[test]
fn an_agent_that_writes_no_patch_fails_the_child() {
    assert_eq!(no_patch().child, FAILED);
}

#[test]
fn an_agent_that_writes_no_patch_says_so() {
    assert!(logged(
        &no_patch(),
        "Model patch file is empty or does not exist"
    ));
}

#[test]
fn an_agent_that_writes_no_chat_fails_the_child() {
    assert_eq!(no_chat().child, FAILED);
}

#[test]
fn an_agent_that_writes_no_chat_cannot_be_copied_out() {
    assert!(logged(&no_chat(), "Error copying from container: "));
}

#[test]
fn an_agent_that_writes_no_chat_is_logged() {
    assert!(logged(&no_chat(), "Error while sampling a child: "));
}

#[test]
fn a_child_that_does_not_build_fails() {
    assert_eq!(child_failure().child, FAILED);
}

#[test]
fn a_child_that_does_not_build_reports_the_compiler() {
    assert!(logged(&child_failure(), "child build failed"));
}

#[test]
fn a_child_that_does_not_build_is_logged() {
    assert!(logged(&child_failure(), "Error while sampling a child: "));
}

fn undiagnosed() -> Attempt {
    attempt(Case {
        commit: "swe",
        overall: Some(performance(&[], &["u"], &[])),
        patch: None,
        polyglot: false,
        replies: vec![reply("no"), reply("no"), reply("no")],
        dataset: vec![json!({"instance_id": "u", "test_patch": "t", "problem_statement": "p"})],
    })
}

#[test]
fn an_undiagnosed_swe_child_fails() {
    assert_eq!(undiagnosed().child, FAILED);
}

#[test]
fn an_undiagnosed_swe_child_says_so() {
    assert!(logged(
        &undiagnosed(),
        "Failed to diagnose the problem statement. Exiting."
    ));
}

#[test]
fn an_undiagnosed_swe_child_records_no_problem_statement() {
    let a = undiagnosed();
    let metadata = read_json(&a.run.as_ref().unwrap().join("metadata.json"));
    assert_eq!(metadata["problem_statement"], Value::Null);
}

#[test]
fn an_undiagnosed_swe_child_asks_three_times() {
    assert_eq!(undiagnosed().requests(), 3);
}
