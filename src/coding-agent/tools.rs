use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

pub const SENTINEL: &str = "<<exit>>";

pub const MAX_VIEW_LENGTH: usize = 10000;

pub fn bash_info() -> Value {
    json!({
        "name": "bash",
        "description": "Run commands in a bash shell\n\n* When invoking this tool, the contents of the \"command\" parameter does NOT need to be XML-escaped.\n\n* You don't have access to the internet via this tool.\n\n* You do have access to a mirror of common linux and python packages via apt and pip.\n\n* State is persistent across command calls and discussions with the user.\n\n* To inspect a particular line range of a file, e.g. lines 10-25, try 'sed -n 10,25p /path/to/the/file'.\n\n* Please avoid commands that may produce a very large amount of output.\n\n* Please run long lived commands in the background, e.g. 'sleep 10 &' or start a server in the background.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The bash command to run."}
            },
            "required": ["command"],
        },
    })
}

pub fn editor_info() -> Value {
    json!({
        "name": "editor",
        "description": "Custom editing tool for viewing, creating, and editing files\n\n* State is persistent across command calls and discussions with the user.\n\n* If `path` is a file, `view` displays the entire file with line numbers. If `path` is a directory, `view` lists non-hidden files and directories up to 2 levels deep.\n\n* The `create` command cannot be used if the specified `path` already exists as a file.\n\n* If a `command` generates a long output, it will be truncated and marked with `<response clipped>`.\n\n* The `edit` command overwrites the entire file with the provided `file_text`.\n\n* No partial/line-range edits or partial viewing are supported.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "enum": ["view", "create", "edit"],
                    "description": "The command to run: `view`, `create`, or `edit`.",
                },
                "path": {
                    "description": "Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.",
                    "type": "string",
                },
                "file_text": {
                    "description": "Required parameter of `create` or `edit` command, containing the content for the entire file.",
                    "type": "string",
                },
            },
            "required": ["command", "path"],
        },
    })
}

pub fn tool_infos() -> Vec<Value> {
    vec![bash_info(), editor_info()]
}

pub fn maybe_truncate(content: &str) -> String {
    match content.char_indices().nth(MAX_VIEW_LENGTH) {
        Some((end, _)) => format!("{}\n<response clipped>", &content[..end]),
        None => content.to_owned(),
    }
}

pub fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let spaces = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(c);
                column = 0;
            }
            _ => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

pub fn format_output(content: &str, path: &str) -> String {
    let content = expand_tabs(&maybe_truncate(content));
    let numbered: Vec<String> = content
        .split('\n')
        .enumerate()
        .map(|(i, line)| format!("{:6}\t{line}", i + 1))
        .collect();
    format!(
        "Here's the result of running `cat -n` on {path}:\n{}\n",
        numbered.join("\n")
    )
}

fn validate_path(path: &str, command: Option<&str>) -> Result<PathBuf, String> {
    let path_obj = PathBuf::from(path);
    if !path.starts_with('/') && !path_obj.is_absolute() {
        return Err(format!(
            "The path {path} is not an absolute path (must start with '/')."
        ));
    }
    match command {
        Some("view") if !path_obj.exists() => Err(format!("The path {path} does not exist.")),
        Some("create") if path_obj.exists() => {
            Err(format!("Cannot create new file; {path} already exists."))
        }
        Some("edit") if !path_obj.exists() => Err(format!("The file {path} does not exist.")),
        Some("edit") if path_obj.is_dir() => Err(format!(
            "{path} is a directory and cannot be edited as a file."
        )),
        Some("view" | "create" | "edit") => Ok(path_obj),
        Some(other) => Err(format!("Unknown or unsupported command: {other}")),
        None => Err("Unknown or unsupported command: None".to_owned()),
    }
}

fn hidden(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, Component::Normal(_) | Component::ParentDir)
            && c.as_os_str().to_string_lossy().starts_with('.')
    })
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) -> std::io::Result<()> {
    let mut names: Vec<PathBuf> = fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        if !hidden(&path) {
            out.push(path.to_string_lossy().into_owned());
        }
        if depth < 2 && path.is_dir() {
            walk(&path, depth + 1, out)?;
        }
    }
    Ok(())
}

fn view_path(path: &Path) -> Result<String, String> {
    let normal: PathBuf = path.components().collect();
    let shown = normal.to_string_lossy();
    let shown = shown.as_ref();
    if path.is_dir() {
        let mut lines = Vec::new();
        if !hidden(&normal) {
            lines.push(shown.to_owned());
        }
        if let Err(e) = walk(Path::new(shown), 1, &mut lines) {
            return Ok(format!("Error listing directory: {e}"));
        }
        return Ok(format!(
            "Here's the files and directories up to 2 levels deep in {shown}, excluding hidden items:\n{}\n",
            lines.join("\n")
        ));
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
    Ok(format_output(&content, shown))
}

fn write_file(path: &Path, content: Option<&str>, command: &str) -> Result<(), String> {
    let content =
        content.ok_or_else(|| format!("Missing required `file_text` for '{command}' command."))?;
    fs::write(path, content).map_err(|e| format!("Failed to write file: {e}"))
}

fn edit(command: Option<&str>, path: &str, file_text: Option<&str>) -> Result<String, String> {
    let path_obj = validate_path(path, command)?;
    match command {
        Some("view") => view_path(&path_obj),
        Some("create") => write_file(&path_obj, file_text, "create")
            .map(|()| format!("File created successfully at: {path}")),
        _ => write_file(&path_obj, file_text, "edit")
            .map(|()| format!("File at {path} has been overwritten with new content.")),
    }
}

pub fn editor(command: Option<&str>, path: &str, file_text: Option<&str>) -> String {
    edit(command, path, file_text).unwrap_or_else(|e| format!("Error: {e}"))
}

pub fn filter_error(error: &str) -> String {
    let lines: Vec<&str> = error.lines().collect();
    let mut filtered = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.contains("Inappropriate ioctl for device") {
            i += 3;
            if lines.get(i).is_some_and(|l| l.contains(SENTINEL)) {
                i += 1;
            }
            while i + 1 < lines.len() {
                filtered.push(lines[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        filtered.push(line);
        i += 1;
    }
    filtered.join("\n").trim().to_owned()
}

#[cfg(windows)]
fn shell() -> PathBuf {
    let system = std::env::var_os("SystemRoot")
        .map(|root| root.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "c:\\windows".into());
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| !dir.to_string_lossy().to_lowercase().starts_with(&system))
        .flat_map(|dir| {
            [
                dir.join("bash.exe"),
                dir.join("..").join("bin").join("bash.exe"),
            ]
        })
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("bash"))
}

#[cfg(not(windows))]
fn shell() -> PathBuf {
    PathBuf::from("/bin/bash")
}

#[derive(Debug, Clone)]
pub struct Bash {
    pub program: PathBuf,
    pub timeout: Duration,
}

impl Default for Bash {
    fn default() -> Self {
        Bash {
            program: shell(),
            timeout: Duration::from_secs(120),
        }
    }
}

enum Chunk {
    Out(Vec<u8>),
    Err(Vec<u8>),
    Closed,
}

fn pump(mut source: Box<dyn Read + Send>, tx: mpsc::Sender<Chunk>, chunk: fn(Vec<u8>) -> Chunk) {
    std::thread::spawn(move || {
        let mut buf = [0; 8192];
        while let Ok(n @ 1..) = source.read(&mut buf) {
            let _ = tx.send(chunk(buf[..n].to_vec()));
        }
        let _ = tx.send(Chunk::Closed);
    });
}

fn before_sentinel(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    text.find(SENTINEL).map(|end| text[..end].trim().to_owned())
}

impl Bash {
    fn timed_out(&self) -> String {
        format!(
            "Timed out: bash has not returned in {:.1} seconds and must be restarted.",
            self.timeout.as_secs_f64()
        )
    }

    pub fn run(&self, command: &str) -> Result<(String, String), String> {
        let mut child = Command::new(&self.program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        pump(
            Box::new(child.stdout.take().unwrap()),
            tx.clone(),
            Chunk::Out,
        );
        pump(Box::new(child.stderr.take().unwrap()), tx, Chunk::Err);
        let script = format!("{command}\necho '{SENTINEL}'; echo '{SENTINEL}' >&2\n");
        let written = child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .map_err(|e| e.to_string());
        let deadline = Instant::now() + self.timeout;
        let (mut out, mut err, mut closed) = (Vec::new(), Vec::new(), 0);
        loop {
            if let (Some(o), Some(e)) = (before_sentinel(&out), before_sentinel(&err)) {
                let _ = child.wait();
                return Ok((o, e));
            }
            if closed == 2 {
                let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
                written?;
                return Err(format!("Bash has exited with returncode {code}"));
            }
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Chunk::Out(data)) => out.extend(data),
                Ok(Chunk::Err(data)) => err.extend(data),
                Ok(Chunk::Closed) => closed += 1,
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(self.timed_out());
                }
            }
        }
    }

    pub fn call(&self, command: &str) -> String {
        match self.run(command) {
            Ok((output, error)) => {
                let error = filter_error(&error);
                let mut result = output;
                if !error.is_empty() {
                    result.push_str("\nError:\n");
                    result.push_str(&error);
                }
                result.trim().to_owned()
            }
            Err(e) => format!("Error: {e}"),
        }
    }
}

fn arguments<'a>(
    input: &'a Value,
    names: &[&str],
    required: usize,
) -> Result<Vec<Option<&'a str>>, String> {
    let map = match input {
        Value::Object(map) => Some(map),
        Value::Null => None,
        other => return Err(format!("arguments must be an object, not {other}")),
    };
    if let Some(extra) = map
        .into_iter()
        .flat_map(Map::keys)
        .find(|k| !names.contains(&k.as_str()))
    {
        return Err(format!("unexpected argument '{extra}'"));
    }
    names
        .iter()
        .enumerate()
        .map(|(i, name)| match map.and_then(|m| m.get(*name)) {
            Some(Value::String(s)) => Ok(Some(s.as_str())),
            Some(Value::Null) => Ok(None),
            None if i >= required => Ok(None),
            None => Err(format!("missing required argument '{name}'")),
            Some(other) => Err(format!("argument '{name}' must be a string, not {other}")),
        })
        .collect()
}

fn null(name: &str) -> String {
    format!("argument '{name}' must be a string, not null")
}

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub bash: Bash,
}

impl Tools {
    fn dispatch(&self, name: &str, input: &Value) -> Result<String, String> {
        match name {
            "bash" => {
                let args = arguments(input, &["command"], 1)?;
                Ok(self.bash.call(args[0].ok_or_else(|| null("command"))?))
            }
            _ => {
                let args = arguments(input, &["command", "path", "file_text"], 2)?;
                Ok(editor(
                    args[0],
                    args[1].ok_or_else(|| null("path"))?,
                    args[2],
                ))
            }
        }
    }

    pub fn call(&self, name: &str, input: &Value) -> String {
        if name != "bash" && name != "editor" {
            return format!("Error: Tool '{name}' not found");
        }
        self.dispatch(name, input)
            .unwrap_or_else(|e| format!("Error executing tool '{name}': {e}"))
    }
}
