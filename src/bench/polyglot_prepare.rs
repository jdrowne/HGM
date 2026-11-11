use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use hgm_agent::git::diff_versus_commit;
use hgm_core::pool;
use hgm_core::pyjson::{PyValue, dumps};
use serde_json::{Map, Value, json};

use crate::polyglot::DATASET;

pub const COMMITS: &str = "polyglot_benchmark_commits.json";
const MARKER: &str = "get_filename_component(exercise ${CMAKE_CURRENT_SOURCE_DIR} NAME)";
const WORKERS: usize = 50;

fn message(e: io::Error) -> String {
    e.to_string()
}

pub type Commits = BTreeMap<String, (String, String)>;

struct Exercise {
    lang: String,
    task: String,
    rel: PathBuf,
    dir: PathBuf,
}

impl Exercise {
    fn id(&self) -> String {
        format!("{}__{}", self.lang, self.task)
    }
}

pub fn modify_cmake_file(path: &Path, exercise: &str) -> io::Result<()> {
    let text = fs::read_to_string(path)?;
    let mut done = false;
    let out: String = text
        .split_inclusive('\n')
        .map(|line| {
            if done || !line.contains(MARKER) {
                return line.to_owned();
            }
            done = true;
            let lead = &line[..line.len() - line.trim_start().len()];
            format!("{lead}set(exercise \"{exercise}\")\n")
        })
        .collect();
    fs::write(path, out)
}

fn visible(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, path)| !name.starts_with('.') && path.is_dir())
        .collect();
    found.sort();
    found
}

fn exercises(root: &Path, bench: &Path) -> Vec<Exercise> {
    visible(&root.join(bench))
        .into_iter()
        .flat_map(|(lang, _)| {
            let practice = bench.join(&lang).join("exercises").join("practice");
            visible(&root.join(&practice))
                .into_iter()
                .map(move |(task, dir)| Exercise {
                    rel: practice.join(&task),
                    lang: lang.clone(),
                    task,
                    dir,
                })
        })
        .collect()
}

fn read_config(dir: &Path) -> Result<Value, String> {
    let path = dir.join(".meta").join("config.json");
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("Config file not found: {}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn listed<'a>(config: &'a Value, category: &str) -> Vec<&'a str> {
    config["files"][category]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(message)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn extra_files(exercise: &Exercise) -> Result<Vec<&'static str>, String> {
    let mut adding = vec![".docs/"];
    match exercise.lang.as_str() {
        "java" => adding.extend(["gradle/", "build.gradle", "gradlew", "gradlew.bat"]),
        "cpp" => {
            modify_cmake_file(&exercise.dir.join("CMakeLists.txt"), &exercise.task)
                .map_err(message)?;
            adding.extend(["test/", "CMakeLists.txt"]);
        }
        "javascript" => adding.extend([".eslintrc", ".npmrc", "package.json"]),
        _ => {}
    }
    Ok(adding)
}

fn commit_exercise(
    exercise: &Exercise,
    solution: &BTreeSet<&str>,
) -> Result<(String, String), String> {
    let dir = &exercise.dir;
    let mut steps: Vec<Vec<&str>> = vec![vec!["init"]];
    steps.extend(solution.iter().map(|file| vec!["add", file]));
    let extra = extra_files(exercise)?;
    steps.extend(
        extra
            .into_iter()
            .filter(|file| dir.join(file).exists())
            .map(|file| vec!["add", file]),
    );
    steps.push(vec!["commit", "-m", "Initial commit"]);
    steps.push(vec!["rev-parse", "HEAD"]);
    let solution_at = steps.len() - 1;
    steps.push(vec!["add", "."]);
    steps.push(vec!["commit", "--amend", "-m", "all files"]);
    steps.push(vec!["rev-parse", "HEAD"]);
    let mut outputs = steps
        .iter()
        .map(|args| git(dir, args))
        .collect::<Result<Vec<String>, String>>()?;
    let all_files_commit = outputs.pop().unwrap_or_default();
    let solution_commit = outputs.swap_remove(solution_at);
    Ok((solution_commit, all_files_commit))
}

type Registered = Option<(String, (String, String))>;

fn init_git_repo(exercise: &Exercise) -> Result<Registered, String> {
    let config = read_config(&exercise.dir)?;
    let solution: BTreeSet<&str> = listed(&config, "solution").into_iter().collect();
    if solution.is_empty() {
        return Err(format!(
            "No solution files found in config for {}",
            exercise.rel.display()
        ));
    }
    Ok(commit_exercise(exercise, &solution)
        .inspect_err(|e| println!("Error initializing git in {}: {e}", exercise.rel.display()))
        .ok()
        .map(|pair| (exercise.id(), pair)))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(path, dumps(&PyValue::from(value), Some(2)))
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn register_git(root: &Path, bench: &Path) -> Result<Commits, String> {
    let pending: Vec<Exercise> = exercises(root, bench)
        .into_iter()
        .filter(|e| !e.dir.join(".git").exists())
        .collect();
    let commits_path = root.join(bench).join(COMMITS);
    if pending.is_empty() {
        println!("All practice directories already initialized with git");
        let text = fs::read_to_string(&commits_path)
            .map_err(|_| format!("Commits file not found: {}", commits_path.display()))?;
        return serde_json::from_str(&text).map_err(|e| format!("{}: {e}", commits_path.display()));
    }
    println!(
        "Initializing git repositories for {} practice directories...",
        pending.len()
    );
    let mut commits = Commits::new();
    for registered in pool::map(&pending, WORKERS, init_git_repo) {
        commits.extend(registered?);
    }
    write_json(&commits_path, &json!(commits))?;
    println!("Saved commit hashes to {}", commits_path.display());
    Ok(commits)
}

fn concatenated(dir: &Path, files: &[&str]) -> Result<String, String> {
    let mut content = String::new();
    for file in files {
        let path = dir.join(file);
        if path.exists() {
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            for part in ["### ", file, "\n", &text, "\n\n"] {
                content.push_str(part);
            }
        }
    }
    Ok(content)
}

fn problem_statement(dir: &Path) -> Result<String, String> {
    let mut docs: Vec<PathBuf> = fs::read_dir(dir.join(".docs"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    docs.sort();
    let mut statement = String::new();
    for doc in docs {
        let text = fs::read_to_string(&doc).map_err(|e| format!("{}: {e}", doc.display()))?;
        statement.push_str(&text);
        statement.push_str("\n\n");
    }
    if statement.is_empty() {
        return Err("Problem statement cannot be empty".to_owned());
    }
    Ok(statement)
}

fn extract_task_metadata(exercise: &Exercise, commits: &Commits) -> Result<Value, String> {
    let id = exercise.id();
    let dir = &exercise.dir;
    let config = read_config(dir)?;
    let mut metadata = Map::new();
    metadata.insert("instance_id".into(), json!(id));
    metadata.insert("language".into(), json!(exercise.lang));
    metadata.insert("task_name".into(), json!(exercise.task));
    metadata.insert("files".into(), json!({}));
    for (key, default) in [
        ("authors", json!([])),
        ("contributors", json!([])),
        ("blurb", json!("")),
        ("source", json!("")),
        ("source_url", json!("")),
    ] {
        metadata.insert(key.into(), config.get(key).cloned().unwrap_or(default));
    }
    if let Some(files) = config.get("files").and_then(Value::as_object) {
        metadata.insert("files".into(), Value::Object(files.clone()));
    }
    metadata.insert("problem_statement".into(), json!(problem_statement(dir)?));
    let (solution_commit, all_files_commit) = commits.get(&id).ok_or_else(|| format!("'{id}'"))?;
    metadata.insert("base_commit".into(), json!(solution_commit));
    metadata.insert("test_commit".into(), json!(all_files_commit));
    let tests = concatenated(dir, &listed(&config, "test"))?;
    metadata.insert("reference_tests".into(), json!(tests));
    let patch = diff_versus_commit(dir, solution_commit).map_err(message)?;
    metadata.insert("test_patch".into(), json!(patch));
    let answers = concatenated(dir, &listed(&config, "example"))?;
    metadata.insert("reference_answers".into(), json!(answers));
    metadata.insert("repo".into(), json!(exercise.rel.to_string_lossy()));
    Ok(Value::Object(metadata))
}

pub fn generate_dataset_metadata(root: &Path, bench: &Path, commits: &Commits) -> Vec<Value> {
    let tasks: Vec<Exercise> = exercises(root, bench)
        .into_iter()
        .filter(|e| e.dir.join(".meta").exists() && e.dir.join(".docs").exists())
        .collect();
    pool::map(&tasks, WORKERS, |exercise| {
        extract_task_metadata(exercise, commits)
            .inspect_err(|e| println!("Error processing {}: {e}", exercise.rel.display()))
            .ok()
    })
    .into_iter()
    .flatten()
    .collect()
}

pub fn prepare(root: &Path, repo_url: &str) -> Result<usize, String> {
    let bench = Path::new("polyglot").join("polyglot-benchmark");
    if root.join(&bench).exists() {
        println!("Found polyglot-benchmark at {}", bench.display());
    } else {
        println!(
            "Cloning polyglot-benchmark repository to {} ...",
            bench.display()
        );
        git(root, &["clone", repo_url, &bench.to_string_lossy()])?;
    }
    let commits = register_git(root, &bench)?;
    let metadata = generate_dataset_metadata(root, &bench, &commits);
    write_json(&root.join(DATASET), &Value::Array(metadata.clone()))?;
    println!("Generated metadata for {} tasks", metadata.len());
    println!("Metadata written to {DATASET}");
    Ok(metadata.len())
}
