use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

pub fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string(value).unwrap()).unwrap();
}

pub fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

pub fn performance(resolved: &[&str], unresolved: &[&str], empty: &[&str]) -> Value {
    let submitted: Vec<&str> = resolved
        .iter()
        .chain(unresolved)
        .chain(empty)
        .copied()
        .collect();
    json!({
        "accuracy_score": 0,
        "total_resolved_instances": resolved.len(),
        "total_submitted_instances": submitted.len(),
        "files": [],
        "total_unresolved_ids": unresolved,
        "total_emptypatch_ids": empty,
        "total_resolved_ids": resolved,
        "total_submitted_ids": submitted,
    })
}

pub fn node(dir: &Path, commit: &str, parent: Option<&str>, overall: Value) {
    let mut metadata = json!({"run_id": commit, "overall_performance": overall});
    if let Some(parent) = parent {
        metadata["parent_commit"] = json!(parent);
    }
    write_json(&dir.join(commit).join("metadata.json"), &metadata);
}

pub fn serve(responses: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for ((status, body), stream) in responses.into_iter().zip(listener.incoming()) {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end().to_ascii_lowercase();
                if header.is_empty() {
                    break;
                }
                if let Some(value) = header.strip_prefix("content-length: ") {
                    length = value.parse().unwrap();
                }
            }
            let mut buf = vec![0; length];
            reader.read_exact(&mut buf).unwrap();
            log.lock().unwrap().push(String::from_utf8(buf).unwrap());
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body.as_bytes()).unwrap();
        }
    });
    (base, seen)
}

pub fn reply(text: &str) -> (u16, String) {
    (
        200,
        json!({"choices": [{"message": {"content": text}}]}).to_string(),
    )
}

pub const DIAGNOSIS: &str =
    "```json\n{\"implementation_suggestion\": \"do it\", \"problem_description\": \"broken\"}\n```";
