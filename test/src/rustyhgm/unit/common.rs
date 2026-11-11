use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::Parser;
use rustyhgm::agent_build::Build;
use rustyhgm::cli::Cli;
use rustyhgm::node::{Bench, Eval};
use rustyhgm_coding_agent::llm::{Provider, Service};
use serde_json::{Value, json};

pub const SETTINGS: [(&str, &str); 29] = [
    ("--self_improve_llm", "si"),
    ("--self_improve_provider", "openai"),
    ("--self_improve_base_url", "http://si/v1"),
    ("--downstream_llm", "ds"),
    ("--downstream_provider", "vllm"),
    ("--downstream_base_url", "http://ds/v1"),
    ("--diagnose_llm", "dg"),
    ("--diagnose_provider", "openrouter"),
    ("--diagnose_base_url", "http://dg/v1"),
    ("--api_key_file", "key"),
    ("--docker", "rustyhgm-no-such-docker-program"),
    ("--swebench_python", "rustyhgm-no-such-python"),
    ("--dataset_endpoint", "http://127.0.0.1:9"),
    ("--dataset_dir", "cache"),
    ("--polyglot_repo", "no-such-repo"),
    ("--polyglot_dir", "polyglot"),
    ("--progenitor_dir", "progenitors"),
    ("--alpha", "0.6"),
    ("--beta", "1.0"),
    ("--eval_random_level", "1.0"),
    ("--n_pseudo_descendant_evals", "10000"),
    ("--expansion_statistic", "clade"),
    ("--epsilon", "0.01"),
    ("--max_workers", "16"),
    ("--max_task_evals", "800"),
    ("--self_improve_timeout", "3600"),
    ("--evaluation_timeout", "3600"),
    ("--output_dir", "out"),
    ("--progenitor_name", "agent"),
];

pub fn argv(changes: &[(&str, &str)], without: &[&str]) -> Vec<String> {
    let mut argv = vec!["rustyhgm".to_owned()];
    let set = SETTINGS
        .iter()
        .filter(|(flag, _)| !without.contains(flag))
        .map(|(flag, value)| {
            let changed = changes.iter().find(|(f, _)| f == flag);
            vec![*flag, changed.map_or(*value, |(_, v)| *v)]
        });
    let extra = changes
        .iter()
        .filter(|(flag, _)| SETTINGS.iter().all(|(f, _)| f != flag))
        .map(|(flag, value)| {
            [*flag, *value]
                .into_iter()
                .filter(|w| !w.is_empty())
                .collect()
        });
    argv.extend(set.chain(extra).flatten().map(str::to_owned));
    argv
}

pub fn cli(changes: &[(&str, &str)]) -> Cli {
    Cli::try_parse_from(argv(changes, &[])).unwrap()
}

pub fn service() -> Service {
    Service {
        provider: Provider::Vllm,
        base_url: "http://h/v1".into(),
        api_key_file: "/no/such/key".into(),
    }
}

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

#[derive(Clone, Debug)]
pub struct Seen {
    pub commit: String,
    pub tasks: Vec<String>,
    pub max_workers: usize,
    pub agent: PathBuf,
}

#[derive(Default)]
pub struct FakeBuild {
    pub fail: bool,
    pub built: Mutex<Vec<(Vec<PathBuf>, PathBuf)>>,
}

impl Build for FakeBuild {
    fn build(&self, patches: &[PathBuf], binary: &Path) -> Result<(), String> {
        self.built
            .lock()
            .unwrap()
            .push((patches.to_vec(), binary.to_path_buf()));
        if self.fail {
            return Err("build failed".to_owned());
        }
        fs::write(binary, "agent").map_err(|e| e.to_string())
    }
}

pub struct FakeBench {
    pub resolved: Vec<String>,
    pub dnames: Option<Vec<PathBuf>>,
    pub fail: bool,
    pub seen: Mutex<Vec<Seen>>,
}

impl FakeBench {
    pub fn resolving(resolved: &[&str]) -> Self {
        FakeBench {
            resolved: resolved.iter().map(|s| (*s).to_owned()).collect(),
            dnames: None,
            fail: false,
            seen: Mutex::default(),
        }
    }
}

impl Bench for FakeBench {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
        self.seen.lock().unwrap().push(Seen {
            commit: eval.commit_id.to_owned(),
            tasks: eval.tasks.to_vec(),
            max_workers: eval.max_workers,
            agent: eval.agent.clone(),
        });
        if self.fail {
            return Err("bench failed".to_owned());
        }
        let resolved: Vec<&String> = eval
            .tasks
            .iter()
            .filter(|t| self.resolved.contains(t))
            .collect();
        let unresolved: Vec<&String> = eval
            .tasks
            .iter()
            .filter(|t| !self.resolved.contains(t))
            .collect();
        let n = fs::read_dir(eval.node_dir).unwrap().count();
        write_json(
            &eval.node_dir.join(format!("{} {n}.json", eval.commit_id)),
            &json!({
                "resolved_instances": resolved.len(),
                "submitted_instances": eval.tasks.len(),
                "resolved_ids": resolved,
                "unresolved_ids": unresolved,
                "empty_patch_ids": [],
                "submitted_ids": eval.tasks,
            }),
        );
        Ok(self.dnames.clone())
    }
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
