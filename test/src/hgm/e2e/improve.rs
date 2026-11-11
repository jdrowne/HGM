use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use hgm_sandbox::source::copy_filtered;
use serde_json::{Value, json};

use super::binary;

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
    assert!(out.status.success(), "{out:?}");
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

fn succeeded(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
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

fn seed_initial_agent(root: &Path) -> Vec<String> {
    let agent = root.join("initial_polyglot/default_agent");
    let tasks = polyglot_tasks(root);
    assert_eq!(tasks[0], TASK);
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
    tasks[1..].to_vec()
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

fn check_records(run: &Path) -> String {
    let records = records(&run.join("hgm_metadata.jsonl"));
    assert_eq!(records.len(), 2, "{records:?}");
    let child = records[0]["nodes"][0]["commit_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let node = |utility: Value, evals: u32| json!([{"commit_id": child, "id": 1, "parent_id": 0, "mean_utility": utility, "num_evals": evals}]);
    assert_eq!(
        records[0],
        json!({"n_task_evals": 0, "nodes": node(json!("Infinity"), 0)})
    );
    assert_eq!(
        records[1],
        json!({"n_task_evals": 1, "nodes": node(json!(0.0), 1)})
    );
    child
}

fn check_child(dir: &Path, unresolved: &[String]) {
    let commit = dir.file_name().unwrap().to_str().unwrap();
    let predictions = dir.join(format!("predictions/{commit}_0"));
    let prediction = read_json(&predictions.join(format!("{TASK}.json")));
    let docker_log =
        fs::read_to_string(predictions.join(format!("{TASK}_docker.log"))).unwrap_or_default();
    let context = format!("{prediction}\n{docker_log}");
    assert_eq!(prediction["eval_result"], "unresolved", "{context}");
    assert!(
        prediction["model_patch"]
            .as_str()
            .is_some_and(|p| p.contains("+++ b/queen-attack.js")),
        "{context}"
    );
    let metadata = read_json(&dir.join("metadata.json"));
    assert_eq!(metadata["parent_commit"], "initial");
    assert!(
        unresolved.iter().any(|t| metadata["entry"] == t.as_str()),
        "{metadata}"
    );
    assert!(
        metadata["problem_statement"]
            .as_str()
            .unwrap()
            .ends_with("# To Implement\n\ndo it\n\nbroken"),
        "{metadata}"
    );
    let overall = &metadata["overall_performance"];
    assert_eq!(overall["accuracy_score"], json!(0.0));
    assert_eq!(overall["total_submitted_ids"], json!([TASK]));
    assert_eq!(overall["total_unresolved_ids"], json!([TASK]));
    assert_eq!(overall["total_resolved_ids"], json!([]));
    let patch = fs::read_to_string(dir.join("model_patch.diff")).unwrap();
    assert!(patch.contains("+++ b/src/agent/lib.rs"), "{patch}");
    assert!(dir.join("hgm-agent").is_file());
}

fn run_args(host: &str) -> Vec<String> {
    let llm = format!("vllm-model-{host}");
    [
        "--polyglot",
        "--max_task_evals",
        "1",
        "--max_workers",
        "1",
        "--eval_random_level",
        "0",
        "--self_improve_llm",
        &llm,
        "--downstream_llm",
        &llm,
        "--diagnose_llm",
        "vllm-model-127.0.0.1",
        "--output_dir",
        "output_hgm/run",
    ]
    .map(str::to_owned)
    .to_vec()
}

#[test]
fn a_run_improves_the_agent_and_measures_its_child() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    copy_repo(root);
    let unresolved = seed_initial_agent(root);
    let seen = serve_llm();
    let stdout = succeeded(&hgm(root).args(run_args(&llm_host())).output().unwrap());
    assert!(stdout.contains("Generated metadata for "), "{stdout}");
    let run = root.join("output_hgm/run");
    assert!(
        stdout.contains(&format!(
            "{}\n",
            Path::new("output_hgm")
                .join("run")
                .join("best_agent")
                .display()
        )),
        "{stdout}"
    );
    let lib = fs::read_to_string(run.join("best_agent/src/agent/lib.rs")).unwrap();
    assert!(lib.ends_with("pub const E2E: u8 = 1;\n"), "{lib}");
    assert_eq!(read_json(&run.join("init_evaluated_tasks.json")), json!([]));
    let child = check_records(&run);
    check_child(&run.join(&child), &unresolved);
    let seen = seen.lock().unwrap();
    for line in [
        "GET /v1/models tools=false last=none",
        "POST /v1/chat/completions tools=false last=user",
        "POST /v1/chat/completions tools=true last=user",
        "POST /v1/chat/completions tools=true last=tool",
    ] {
        assert!(seen.iter().any(|s| s == line), "{line}: {seen:?}");
    }
}
