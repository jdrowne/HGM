use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

type Seen = Arc<Mutex<Vec<Value>>>;

fn agent() -> Command {
    Command::new(
        std::env::var_os("NEXTEST_BIN_EXE_hgm-agent")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_hgm-agent").into()),
    )
}

fn completion(body: &Value) -> Value {
    let messages = body["messages"].as_array().unwrap();
    let message = if messages.last().unwrap()["role"] == "tool" {
        json!({"role": "assistant", "content": "done"})
    } else {
        let dir = messages[1]["content"]
            .as_str()
            .unwrap()
            .split("in the directory ")
            .nth(1)
            .unwrap()
            .split(". Help")
            .next()
            .unwrap()
            .to_owned();
        let command = format!("cd {dir} && echo new > added.txt");
        json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": {"name": "bash", "arguments": json!({"command": command}).to_string()},
        }]})
    };
    json!({"choices": [{"message": message}]})
}

fn answer(stream: TcpStream, seen: &Mutex<Vec<Value>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let header = header.trim_end().to_ascii_lowercase();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
    }
    let mut buf = vec![0; length];
    reader.read_exact(&mut buf).unwrap();
    let reply = if line.starts_with("GET") {
        json!({"data": [{"id": "served"}]})
    } else {
        let body: Value = serde_json::from_slice(&buf).unwrap();
        seen.lock().unwrap().push(body.clone());
        completion(&body)
    }
    .to_string();
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    )
    .unwrap();
}

fn serve() -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen = Seen::default();
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            answer(stream, &log);
        }
    });
    (base, seen)
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

fn repo(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    repo
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn the_agent_edits_the_repo_and_writes_its_patch_and_chat_log() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let (base, seen) = serve();
    let out = agent()
        .env("HGM_LLM_BASE_URL", &base)
        .args(["--problem_statement", "fix it", "--base_commit", "HEAD"])
        .arg("--git_dir")
        .arg(&repo)
        .arg("--chat_history_file")
        .arg(tmp.path().join("logs/chat.md"))
        .arg("--outdir")
        .arg(tmp.path())
        .args(["--model", "vllm-x", "--test_description", "run it"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let patch = fs::read_to_string(tmp.path().join("model_patch.diff")).unwrap();
    assert!(patch.contains("+++ b/added.txt"), "{patch}");
    let chat = fs::read_to_string(tmp.path().join("logs/chat.md")).unwrap();
    assert!(
        chat.starts_with(
            "\n========== User Instruction ==========\nI have uploaded a Python code repository"
        ),
        "{chat}"
    );
    assert!(chat.contains("Tool Used: bash\n"), "{chat}");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let instruction = seen[0]["messages"][1]["content"].as_str().unwrap();
    assert!(instruction.contains("<test_description>\nrun it\n</test_description>"));
    assert_eq!(seen[0]["model"], "served");
}

#[test]
fn polyglot_runs_use_their_own_instruction_and_relative_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let (base, seen) = serve();
    let out = agent()
        .current_dir(tmp.path())
        .env("HGM_LLM_BASE_URL", &base)
        .args(["--problem_statement", "fix it", "--base_commit", "HEAD"])
        .arg("--git_dir")
        .arg(&repo)
        .args(["--chat_history_file", "chat.md", "--outdir", ""])
        .args(["--polyglot", "--language", "rust", "--model", "vllm-x"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(tmp.path().join("chat.md").is_file());
    let patch = fs::read_to_string(tmp.path().join("model_patch.diff")).unwrap();
    assert!(patch.contains("+++ b/added.txt"), "{patch}");
    let instruction = seen.lock().unwrap()[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(instruction.starts_with("I have uploaded a code repository in the directory "));
    assert!(
        instruction.ends_with("Then make edits to the code files to complete the instruction.")
    );
}

fn failing(tmp: &Path, chat: &Path, outdir: &Path) -> Command {
    let mut cmd = agent();
    cmd.env("HGM_LLM_BASE_URL", "http://127.0.0.1:9/v1")
        .args([
            "--problem_statement",
            "p",
            "--base_commit",
            "HEAD",
            "--model",
            "qwen",
        ])
        .arg("--git_dir")
        .arg(tmp)
        .arg("--chat_history_file")
        .arg(chat)
        .arg("--outdir")
        .arg(outdir);
    cmd
}

#[test]
fn unwritable_outputs_and_a_missing_git_fail_the_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    let out = failing(tmp.path(), &file.join("chat.md"), tmp.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("hgm-agent: "));
    let chat = tmp.path().join("chat.md");
    let out = failing(tmp.path(), &chat, &file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        fs::read_to_string(&chat)
            .unwrap()
            .contains("Error in chat_with_agent_open_router: ")
    );
    let out = failing(tmp.path(), &chat, tmp.path())
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("hgm-agent: "));
}

#[test]
fn the_agent_without_its_required_flags_exits_with_usage_status() {
    let out = agent().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--problem_statement"));
    let out = agent().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("hgm-agent ")
    );
}
