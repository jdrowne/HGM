use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use hgm_sandbox::source::copy_filtered;
use serde_json::{Value, json};

use super::{binary, flags};

const TASK: &str = "javascript__queen-attack";
const IMPROVE_COMMAND: &str =
    "cd /hgm && printf 'pub const E2E: u8 = 1;\\n' >> src/agent/lib.rs && echo ok";
const SOLVE_COMMAND: &str =
    "cd /testbed && printf 'export const e2e = 1;\\n' >> queen-attack.js && echo ok";
const DIAGNOSIS: &str = "```json\n{\"log_summarization\": \"s\", \"potential_improvements\": [\"p\"], \"improvement_proposal\": \"i\", \"implementation_suggestion\": \"do it\", \"problem_description\": \"broken\"}\n```";

type Seen = Arc<Mutex<Vec<String>>>;

fn llm_host() -> String {
    std::env::var("HGM_E2E_LLM_HOST").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "172.17.0.1"
        } else {
            "host.docker.internal"
        }
        .to_owned()
    })
}

fn completion(body: &Value) -> Value {
    let tools = body["tools"].as_array().is_some_and(|t| !t.is_empty());
    let after_tool = body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .is_some_and(|m| m["role"] == "tool");
    let command = if body["messages"].to_string().contains("/testbed") {
        SOLVE_COMMAND
    } else {
        IMPROVE_COMMAND
    };
    let (message, reason) = match (tools, after_tool) {
        (true, false) => (
            json!({"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "bash", "arguments": json!({"command": command}).to_string()},
            }]}),
            "tool_calls",
        ),
        (true, true) => (json!({"role": "assistant", "content": "done"}), "stop"),
        (false, _) => (json!({"role": "assistant", "content": DIAGNOSIS}), "stop"),
    };
    json!({
        "id": "x",
        "object": "chat.completion",
        "created": 0,
        "model": "fake",
        "choices": [{"index": 0, "message": message, "finish_reason": reason}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
    })
}

fn summary(method: &str, path: &str, body: &Value) -> String {
    let tools = body["tools"].as_array().is_some_and(|t| !t.is_empty());
    let last = body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["role"].as_str())
        .unwrap_or("none");
    format!("{method} {path} tools={tools} last={last}")
}

fn answer(stream: TcpStream, seen: &Mutex<Vec<String>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut words = request.split_whitespace();
    let method = words.next().unwrap_or_default().to_owned();
    let path = words.next().unwrap_or_default().to_owned();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end().to_ascii_lowercase();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut buf = vec![0; length];
    reader.read_exact(&mut buf)?;
    let body: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
    seen.lock().unwrap().push(summary(&method, &path, &body));
    let reply = if method == "GET" {
        json!({"object": "list", "data": [{"id": "fake", "object": "model", "created": 0, "owned_by": "x"}]})
    } else {
        completion(&body)
    }
    .to_string();
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    )
}

fn serve_llm() -> Seen {
    let listener = TcpListener::bind("0.0.0.0:8000").expect("bind the fake LLM to port 8000");
    let seen = Seen::default();
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let log = Arc::clone(&log);
            std::thread::spawn(move || answer(stream, &log));
        }
    });
    seen
}

fn repo_root() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
        .join("../..")
}

fn copy_repo(root: &Path) {
    let repo = repo_root();
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["ls-files", "-z"])
        .output()
        .unwrap();
    if !out.status.success() {
        panic!("git ls-files failed: {out:?}");
    }
    for name in out.stdout.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let name = std::str::from_utf8(name).unwrap();
        let from = repo.join(name);
        if from.is_file() {
            let to = root.join(name);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::copy(&from, &to).unwrap();
        }
    }
}

fn hgm(root: &Path) -> Command {
    let mut cmd = Command::new(binary("hgm"));
    cmd.current_dir(root);
    for key in ["AUTHOR", "COMMITTER"] {
        cmd.env(format!("GIT_{key}_NAME"), "hgm")
            .env(format!("GIT_{key}_EMAIL"), "hgm@example.com")
            .env(format!("GIT_{key}_DATE"), "2024-01-01T00:00:00Z");
    }
    cmd
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn polyglot_tasks(root: &Path) -> Vec<String> {
    let mut tasks: Vec<String> = Vec::new();
    for subset in ["medium", "small"] {
        let path = root.join(format!("src/bench/subsets/polyglot/{subset}.json"));
        for task in serde_json::from_value::<Vec<String>>(read_json(&path)).unwrap() {
            if !tasks.contains(&task) {
                tasks.push(task);
            }
        }
    }
    tasks
}

fn seed_initial_agent(root: &Path) {
    let agent = root.join("initial_polyglot/default_agent");
    let tasks = polyglot_tasks(root);
    if tasks[0] != TASK {
        panic!("the first polyglot task is {}, not {TASK}", tasks[0]);
    }
    fs::create_dir_all(agent.join("predictions")).unwrap();
    let metadata = json!({"run_id": "initial", "overall_performance": {
        "accuracy_score": 1.0 / tasks.len() as f64,
        "total_resolved_instances": 1,
        "total_submitted_instances": tasks.len(),
        "files": [],
        "total_unresolved_ids": tasks[1..],
        "total_emptypatch_ids": [],
        "total_resolved_ids": tasks[..1],
        "total_submitted_ids": tasks,
    }});
    fs::write(agent.join("metadata.json"), metadata.to_string()).unwrap();
    copy_filtered(root, &agent.join("src")).unwrap();
}

fn records(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path)
        .unwrap()
        .replace("Infinity", "\"Infinity\"");
    serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .map(Result::unwrap)
        .collect()
}

fn run_args(host: &str, key: &str) -> Vec<String> {
    let agent_url = format!("http://{host}:8000/v1");
    flags(&[
        ("--api_key_file", key),
        ("--docker", "docker"),
        ("--polyglot", "true"),
        ("--max_task_evals", "1"),
        ("--max_workers", "1"),
        ("--eval_random_level", "0"),
        ("--self_improve_llm", "vllm-model"),
        ("--self_improve_provider", "vllm"),
        ("--self_improve_base_url", &agent_url),
        ("--downstream_llm", "vllm-model"),
        ("--downstream_provider", "vllm"),
        ("--downstream_base_url", &agent_url),
        ("--diagnose_llm", "vllm-model"),
        ("--diagnose_provider", "vllm"),
        ("--diagnose_base_url", "http://127.0.0.1:8000/v1"),
        ("--initial_agent_name", "default_agent"),
        ("--output_dir", "output_hgm/run"),
    ])
}

fn once(name: &str, build: impl FnOnce(&Path)) -> PathBuf {
    let run = std::env::var("NEXTEST_RUN_ID").unwrap_or_else(|_| std::process::id().to_string());
    let dir = std::env::temp_dir().join(format!("hgm-e2e-{name}-{run}"));
    fs::create_dir_all(&dir).unwrap();
    let lock = fs::File::create(dir.join("lock")).unwrap();
    lock.lock().unwrap();
    let done = dir.join("done");
    if let Ok(root) = fs::read_to_string(&done) {
        return PathBuf::from(root);
    }
    let root = tempfile::Builder::new()
        .prefix("attempt")
        .tempdir_in(&dir)
        .unwrap()
        .keep();
    build(&root);
    fs::write(&done, root.to_string_lossy().as_bytes()).unwrap();
    root
}

fn scenario(root: &Path) {
    copy_repo(root);
    seed_initial_agent(root);
    let seen = serve_llm();
    let key = root.with_extension("key");
    fs::write(&key, "k\n").unwrap();
    let out: Output = hgm(root)
        .args(run_args(&llm_host(), &key.to_string_lossy()))
        .output()
        .unwrap();
    let outcome = json!({
        "code": out.status.code(),
        "stdout": String::from_utf8_lossy(&out.stdout),
        "stderr": String::from_utf8_lossy(&out.stderr),
        "seen": *seen.lock().unwrap(),
    });
    fs::write(root.join("outcome.json"), outcome.to_string()).unwrap();
}

struct Shared {
    root: PathBuf,
    outcome: Value,
}

fn shared() -> Shared {
    let root = once("improve", scenario);
    let outcome = read_json(&root.join("outcome.json"));
    Shared { root, outcome }
}

impl Shared {
    fn stdout(&self) -> &str {
        self.outcome["stdout"].as_str().unwrap()
    }

    fn run(&self) -> PathBuf {
        self.root.join("output_hgm/run")
    }

    fn records(&self) -> Vec<Value> {
        records(&self.run().join("hgm_metadata.jsonl"))
    }

    fn child(&self) -> String {
        self.records()[0]["nodes"][0]["commit_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn child_dir(&self) -> PathBuf {
        self.run().join(self.child())
    }

    fn node(&self, utility: Value, evals: u32) -> Value {
        json!([{"commit_id": self.child(), "id": 1, "parent_id": 0, "mean_utility": utility, "num_evals": evals}])
    }

    fn prediction(&self) -> Value {
        read_json(
            &self
                .child_dir()
                .join(format!("predictions/{}_0/{TASK}.json", self.child())),
        )
    }

    fn metadata(&self) -> Value {
        read_json(&self.child_dir().join("metadata.json"))
    }
}

#[test]
fn a_run_improves_the_agent_and_measures_its_child() {
    let s = shared();
    assert_eq!(
        s.outcome["code"],
        json!(0),
        "{}{}",
        s.stdout(),
        s.outcome["stderr"]
    );
}

#[test]
fn the_run_prepares_the_polyglot_dataset() {
    assert!(shared().stdout().contains("Generated metadata for "));
}

#[test]
fn the_run_prints_where_the_best_agent_is() {
    let best = Path::new("output_hgm").join("run").join("best_agent");
    assert!(shared().stdout().contains(&format!("{}\n", best.display())));
}

#[test]
fn the_best_agent_carries_the_improvement() {
    let lib = fs::read_to_string(shared().run().join("best_agent/src/agent/lib.rs")).unwrap();
    assert!(lib.ends_with("pub const E2E: u8 = 1;\n"), "{lib}");
}

#[test]
fn the_initial_agent_needs_no_evaluation() {
    assert_eq!(
        read_json(&shared().run().join("init_evaluated_tasks.json")),
        json!([])
    );
}

#[test]
fn the_run_records_two_states() {
    assert_eq!(shared().records().len(), 2);
}

#[test]
fn the_first_record_holds_the_unmeasured_child() {
    let s = shared();
    assert_eq!(
        s.records()[0],
        json!({"n_task_evals": 0, "nodes": s.node(json!("Infinity"), 0)})
    );
}

#[test]
fn the_second_record_holds_the_measured_child() {
    let s = shared();
    assert_eq!(
        s.records()[1],
        json!({"n_task_evals": 1, "nodes": s.node(json!(0.0), 1)})
    );
}

#[test]
fn the_child_leaves_its_task_unresolved() {
    assert_eq!(shared().prediction()["eval_result"], "unresolved");
}

#[test]
fn the_child_patches_its_task() {
    assert!(
        shared().prediction()["model_patch"]
            .as_str()
            .is_some_and(|p| p.contains("+++ b/queen-attack.js"))
    );
}

#[test]
fn the_child_names_its_parent() {
    assert_eq!(shared().metadata()["parent_commit"], "initial");
}

#[test]
fn the_child_works_on_an_unresolved_entry() {
    let s = shared();
    let unresolved = polyglot_tasks(&s.root)[1..].to_vec();
    let entry = s.metadata()["entry"].clone();
    assert!(unresolved.iter().any(|t| entry == t.as_str()), "{entry}");
}

#[test]
fn the_child_records_the_diagnosis() {
    assert!(
        shared().metadata()["problem_statement"]
            .as_str()
            .unwrap()
            .ends_with("# To Implement\n\ndo it\n\nbroken")
    );
}

fn overall(key: &str) -> Value {
    shared().metadata()["overall_performance"][key].clone()
}

#[test]
fn the_child_scores_zero() {
    assert_eq!(overall("accuracy_score"), json!(0.0));
}

#[test]
fn the_child_submits_its_task() {
    assert_eq!(overall("total_submitted_ids"), json!([TASK]));
}

#[test]
fn the_child_counts_its_task_unresolved() {
    assert_eq!(overall("total_unresolved_ids"), json!([TASK]));
}

#[test]
fn the_child_resolves_nothing() {
    assert_eq!(overall("total_resolved_ids"), json!([]));
}

#[test]
fn the_child_keeps_its_agent_patch() {
    let patch = fs::read_to_string(shared().child_dir().join("model_patch.diff")).unwrap();
    assert!(patch.contains("+++ b/src/agent/lib.rs"), "{patch}");
}

#[test]
fn the_child_keeps_its_built_agent() {
    assert!(shared().child_dir().join("hgm-agent").is_file());
}

#[test]
fn the_fake_llm_sees_each_kind_of_request() {
    let s = shared();
    let seen: Vec<String> = serde_json::from_value(s.outcome["seen"].clone()).unwrap();
    let missing: Vec<&str> = [
        "GET /v1/models tools=false last=none",
        "POST /v1/chat/completions tools=false last=user",
        "POST /v1/chat/completions tools=true last=user",
        "POST /v1/chat/completions tools=true last=tool",
    ]
    .into_iter()
    .filter(|line| !seen.iter().any(|s| s == line))
    .collect();
    assert_eq!(missing, Vec::<&str>::new(), "{seen:?}");
}
