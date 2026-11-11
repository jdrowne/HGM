use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

type Seen = Arc<Mutex<Vec<Value>>>;

fn harness() -> Command {
    Command::new(
        std::env::var_os("NEXTEST_BIN_EXE_rustyhgm-coding-harness")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_rustyhgm-coding-harness").into()),
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
        let command = format!("cd '{dir}' && echo new > added.txt");
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
    if !out.status.success() {
        panic!("{out:?}");
    }
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

fn explicit<'a>(cmd: &'a mut Command, root: &Path, provider: &str, base: &str) -> &'a mut Command {
    let key = root.join("key");
    fs::write(&key, "k\n").unwrap();
    cmd.args(["--timeout", "3600"])
        .args(["--provider", provider, "--base_url", base])
        .arg("--api_key_file")
        .arg(key)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

struct Ran {
    tmp: tempfile::TempDir,
    out: Output,
    seen: Seen,
}

impl Ran {
    fn patch(&self) -> String {
        fs::read_to_string(self.tmp.path().join("model_patch.diff")).unwrap()
    }

    fn instruction(&self) -> String {
        self.seen.lock().unwrap()[0]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

fn edited() -> Ran {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let (base, seen) = serve();
    let out = explicit(&mut harness(), tmp.path(), "vllm", &base)
        .args(["--problem_statement", "fix it", "--base_commit", "HEAD"])
        .arg("--git_dir")
        .arg(&repo)
        .arg("--chat_history_file")
        .arg(tmp.path().join("logs/chat.md"))
        .arg("--outdir")
        .arg(tmp.path())
        .args(["--model", "vllm-x", "--test_description", "run it"])
        .args(["--language", "python"])
        .output()
        .unwrap();
    Ran { tmp, out, seen }
}

fn chat_log(ran: &Ran) -> String {
    fs::read_to_string(ran.tmp.path().join("logs/chat.md")).unwrap()
}

#[test]
fn the_harness_succeeds() {
    let ran = edited();
    assert!(ran.out.status.success(), "{}", stderr(&ran.out));
}

#[test]
fn the_harness_writes_its_patch() {
    let patch = edited().patch();
    assert!(patch.contains("+++ b/added.txt"), "{patch}");
}

#[test]
fn the_chat_log_starts_with_the_instruction() {
    let chat = chat_log(&edited());
    assert!(
        chat.starts_with(
            "\n========== User Instruction ==========\nI have uploaded a Python code repository"
        ),
        "{chat}"
    );
}

#[test]
fn the_chat_log_records_the_tool_used() {
    let chat = chat_log(&edited());
    assert!(chat.contains("Tool Used: bash\n"), "{chat}");
}

#[test]
fn the_harness_asks_the_model_twice() {
    assert_eq!(edited().seen.lock().unwrap().len(), 2);
}

#[test]
fn the_instruction_carries_the_test_description() {
    assert!(
        edited()
            .instruction()
            .contains("<test_description>\nrun it\n</test_description>")
    );
}

#[test]
fn the_harness_asks_for_the_served_model() {
    assert_eq!(edited().seen.lock().unwrap()[0]["model"], "served");
}

fn polyglot() -> Ran {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let (base, seen) = serve();
    let out = explicit(&mut harness(), tmp.path(), "vllm", &base)
        .current_dir(tmp.path())
        .args(["--problem_statement", "fix it", "--base_commit", "HEAD"])
        .arg("--git_dir")
        .arg(&repo)
        .args(["--chat_history_file", "chat.md", "--outdir", ""])
        .args(["--polyglot", "--language", "rust", "--model", "vllm-x"])
        .args(["--test_description", "run it"])
        .output()
        .unwrap();
    Ran { tmp, out, seen }
}

#[test]
fn a_polyglot_run_succeeds() {
    let ran = polyglot();
    assert!(ran.out.status.success(), "{}", stderr(&ran.out));
}

#[test]
fn a_polyglot_chat_log_lands_relative_to_the_working_directory() {
    assert!(polyglot().tmp.path().join("chat.md").is_file());
}

#[test]
fn a_polyglot_patch_lands_relative_to_the_working_directory() {
    let patch = polyglot().patch();
    assert!(patch.contains("+++ b/added.txt"), "{patch}");
}

#[test]
fn a_polyglot_instruction_opens_with_the_code_repository() {
    assert!(
        polyglot()
            .instruction()
            .starts_with("I have uploaded a code repository in the directory ")
    );
}

#[test]
fn a_polyglot_instruction_ends_with_the_edit_request() {
    assert!(
        polyglot()
            .instruction()
            .ends_with("Then make edits to the code files to complete the instruction.")
    );
}

fn failing(tmp: &Path, chat: &Path, outdir: &Path) -> Command {
    let mut cmd = harness();
    explicit(&mut cmd, tmp, "openrouter", "http://127.0.0.1:9/v1")
        .args([
            "--problem_statement",
            "p",
            "--base_commit",
            "HEAD",
            "--model",
            "qwen",
        ])
        .args(["--language", "python"])
        .args(["--test_description", "t"])
        .arg("--git_dir")
        .arg(tmp)
        .arg("--chat_history_file")
        .arg(chat)
        .arg("--outdir")
        .arg(outdir);
    cmd
}

fn with_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    fs::write(&file, "").unwrap();
    (tmp, file)
}

fn unwritable_chat() -> Output {
    let (tmp, file) = with_file();
    failing(tmp.path(), &file.join("chat.md"), tmp.path())
        .output()
        .unwrap()
}

#[test]
fn an_unwritable_chat_log_fails_the_harness() {
    assert_eq!(unwritable_chat().status.code(), Some(1));
}

#[test]
fn an_unwritable_chat_log_is_reported() {
    assert!(stderr(&unwritable_chat()).starts_with("rustyhgm-coding-harness: "));
}

fn unwritable_outdir() -> (tempfile::TempDir, Output) {
    let (tmp, file) = with_file();
    let out = failing(tmp.path(), &tmp.path().join("chat.md"), &file)
        .output()
        .unwrap();
    (tmp, out)
}

#[test]
fn an_unwritable_outdir_fails_the_harness() {
    let (_tmp, out) = unwritable_outdir();
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
}

#[test]
fn an_unreachable_model_is_logged_in_the_chat() {
    let (tmp, _) = unwritable_outdir();
    assert!(
        fs::read_to_string(tmp.path().join("chat.md"))
            .unwrap()
            .contains("Error in chat_with_tools_open_router: ")
    );
}

fn without_git() -> Output {
    let tmp = tempfile::tempdir().unwrap();
    failing(tmp.path(), &tmp.path().join("chat.md"), tmp.path())
        .env("PATH", "")
        .output()
        .unwrap()
}

#[test]
fn a_missing_git_fails_the_harness() {
    assert_eq!(without_git().status.code(), Some(1));
}

#[test]
fn a_missing_git_is_reported() {
    assert!(stderr(&without_git()).starts_with("rustyhgm-coding-harness: "));
}

fn keyless() -> (tempfile::TempDir, Output) {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = failing(tmp.path(), &tmp.path().join("chat.md"), tmp.path());
    fs::remove_file(tmp.path().join("key")).unwrap();
    let out = cmd.output().unwrap();
    (tmp, out)
}

#[test]
fn a_missing_key_file_fails_the_harness() {
    assert_eq!(keyless().1.status.code(), Some(1));
}

#[test]
fn a_missing_key_file_is_reported_by_name() {
    let (tmp, out) = keyless();
    assert!(stderr(&out).starts_with(&format!(
        "rustyhgm-coding-harness: {}: ",
        tmp.path().join("key").display()
    )));
}

#[test]
fn a_missing_key_file_stops_the_harness_before_the_chat_log() {
    assert!(!keyless().0.path().join("chat.md").exists());
}

#[test]
fn the_harness_without_its_required_flags_exits_with_usage_status() {
    assert_eq!(harness().output().unwrap().status.code(), Some(2));
}

#[test]
fn the_usage_names_the_required_flags() {
    assert!(stderr(&harness().output().unwrap()).contains("--problem_statement"));
}

fn version() -> Output {
    harness().arg("--version").output().unwrap()
}

#[test]
fn the_version_flag_succeeds() {
    assert!(version().status.success());
}

#[test]
fn the_version_names_the_harness() {
    assert!(
        String::from_utf8(version().stdout)
            .unwrap()
            .starts_with("rustyhgm-coding-harness ")
    );
}
