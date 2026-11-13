use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use clap::{Parser, ValueEnum};

use crate::git::diff_versus_commit;
use crate::llm::{Client, Provider, Service, create_client};
use crate::withtools::Chat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    Cpp,
    Java,
    Python,
    Go,
    Rust,
    Javascript,
}

#[derive(Debug, Parser)]
#[command(
    name = "rustyhgm-coding-harness",
    version,
    about = "Coding harness that works on a repository through a language model."
)]
pub struct Args {
    #[arg(long = "problem_statement")]
    pub problem_statement: String,
    #[arg(long = "git_dir")]
    pub git_dir: PathBuf,
    #[arg(long = "base_commit")]
    pub base_commit: String,
    #[arg(long = "chat_history_file")]
    pub chat_history_file: PathBuf,
    #[arg(long)]
    pub outdir: String,
    #[arg(long = "test_description")]
    pub test_description: String,
    #[arg(long = "self_improve")]
    pub self_improve: bool,
    #[arg(long = "instance_id")]
    pub instance_id: Option<String>,
    #[arg(long)]
    pub polyglot: bool,
    #[arg(long, value_enum)]
    pub language: Language,
    #[arg(long)]
    pub model: String,
    #[arg(long, value_enum)]
    pub provider: Provider,
    #[arg(long = "base_url")]
    pub base_url: String,
    #[arg(long = "api_key_file")]
    pub api_key_file: PathBuf,
    #[arg(long)]
    pub timeout: i64,
}

impl Args {
    pub fn service(&self) -> Service {
        Service {
            provider: self.provider,
            base_url: self.base_url.clone(),
            api_key_file: self.api_key_file.clone(),
        }
    }

    pub fn instruction(&self) -> String {
        let dir = self.git_dir.display();
        let problem = &self.problem_statement;
        if self.polyglot {
            return format!(
                "I have uploaded a code repository in the directory {dir}. Help solve the following problem.\n\n<problem_description>\n{problem}\n</problem_description>\n\nYour task is to make changes to the files in the {dir} directory to address the <problem_description>. I have already taken care of the required dependencies.\n\n\nPlease analyze the problem description carefully. Then make edits to the code files to complete the instruction."
            );
        }
        let tests = &self.test_description;
        format!(
            "I have uploaded a Python code repository in the directory {dir}. Help solve the following problem.\n\n<problem_description>\n{problem}\n</problem_description>\n\n<test_description>\n{tests}\n</test_description>\n\nYour task is to make changes to the files in the {dir} directory to address the <problem_description>. I have already taken care of the required dependencies.\n"
        )
    }

    pub fn patch_file(&self) -> PathBuf {
        if self.outdir.is_empty() {
            PathBuf::from("model_patch.diff")
        } else {
            Path::new(&self.outdir).join("model_patch.diff")
        }
    }
}

pub struct ChatLog {
    file: Mutex<File>,
}

impl ChatLog {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(ChatLog {
            file: Mutex::new(file),
        })
    }

    pub fn log(&self, message: &str) {
        let mut file = self.file.lock().unwrap();
        let _ = writeln!(file, "{message}");
    }
}

pub fn forward(args: &Args, client: Client, log: &ChatLog) {
    let start = Instant::now();
    let sink = |m: &str| log.log(m);
    let mut chat = Chat::new(client, &sink);
    chat.timeout = (args.timeout - 60) as f64 - start.elapsed().as_secs_f64();
    chat.run(&args.instruction(), &[]);
}

pub fn run(args: &Args) -> Result<(), String> {
    let client = create_client(&args.model, &args.service())?;
    let log = ChatLog::create(&args.chat_history_file).map_err(|e| e.to_string())?;
    forward(args, client, &log);
    diff_versus_commit(&args.git_dir, &args.base_commit)
        .and_then(|patch| fs::write(args.patch_file(), patch))
        .map_err(|e| e.to_string())
}
