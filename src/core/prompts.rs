use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use rand::{Rng, RngExt};
use regex::Regex;
use serde_json::Value;

pub const CODING_AGENT_SUMMARY: &str = include_str!("prompts/coding_agent_summary.txt");
pub const CODING_AGENT_SUMMARY_POLYGLOT: &str =
    include_str!("prompts/coding_agent_summary_polyglot.txt");
pub const DIAGNOSE_SYSTEM_MESSAGE: &str = include_str!("prompts/diagnose_system_message.txt");
pub const SWE_ISSUE_PROMPT: &str = include_str!("prompts/swe_issue_prompt.txt");
pub const POLYGLOT_ISSUE_PROMPT: &str = include_str!("prompts/polyglot_issue_prompt.txt");
pub const DIAGNOSE_PROMPT: &str = include_str!("prompts/diagnose_prompt.txt");
pub const DIAGNOSE_PROMPT_EMPTYPATCHES: &str =
    include_str!("prompts/diagnose_prompt_emptypatches.txt");
pub const DIAGNOSE_PROMPT_STOCHASTICITY: &str =
    include_str!("prompts/diagnose_prompt_stochasticity.txt");
pub const DIAGNOSE_PROMPT_CONTEXTLENGTH: &str =
    include_str!("prompts/diagnose_prompt_contextlength.txt");
pub const PROBLEM_DESCRIPTION_PROMPT: &str = include_str!("prompts/problem_description_prompt.txt");
pub const DIAGNOSE_PROMPT_EMPTYPATCHES_POLYGLOT: &str =
    include_str!("prompts/diagnose_prompt_emptypatches_polyglot.txt");
pub const DIAGNOSE_PROMPT_STOCHASTICITY_POLYGLOT: &str =
    include_str!("prompts/diagnose_prompt_stochasticity_polyglot.txt");

pub const SOLVE_EMPTY_PATCHES: &str = "solve_empty_patches";
pub const SOLVE_STOCHASTICITY: &str = "solve_stochasticity";
pub const SOLVE_CONTEXTLENGTH: &str = "solve_contextlength";

pub const MD_LOG_LIMIT: usize = 100_000;

pub const NO_LOGS: &str = "No logs available.";
pub const NO_TEST_RESULTS: &str = "No test results available. Assume all tests failed.";
pub const NO_PATCH: &str = "No predicted patch available. Assume the agent failed.";
pub const NO_EVAL_RESULT: &str = "No evaluation result available. Assume the agent failed.";

pub const FILTERED_LOG_PREFIX: &str = "Error in get_response_withtools";

pub const CODE_FILES: [&str; 1] = ["src/agent/"];

pub const CONTEXT_LENGTH_ERRORS: [&str; 3] = [
    r"Error in get_response_withtools: Error code: 400 - {'message': 'Input is too long for requested model.'}",
    r#"Error in get_response_withtools: Error code: 400 - {'object': 'error', 'message': \"This model's maximum context length is \d+ tokens. However, you requested \d+ tokens in the messages, Please reduce the length of the messages. None\", 'type': 'BadRequestError', 'param': None, 'code': 400}"#,
    r"Error in get_response_withtools: Error code: 400 - {'error': {'message': 'Your input exceeds the context window of this model. Please adjust your input and try again.', 'type': 'invalid_request_error', 'param': 'input', 'code': 'context_length_exceeded'}}",
];

pub fn format_named(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let key = after.find('}').map(|close| &after[..close]);
        match key.and_then(|k| values.iter().find(|(name, _)| *name == k)) {
            Some((name, value)) => {
                out.push_str(value);
                rest = &after[name.len() + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn read_text(path: &Path) -> io::Result<String> {
    let text = fs::read_to_string(path)?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

pub fn read_file(path: &Path) -> io::Result<String> {
    Ok(read_text(path)?.trim().to_owned())
}

pub fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn get_problem_description_prompt(response: &Value, polyglot: bool) -> Result<String, String> {
    let field = |k: &str| {
        response
            .get(k)
            .map(value_text)
            .ok_or_else(|| format!("missing {k} in the diagnosis"))
    };
    let summary = if polyglot {
        CODING_AGENT_SUMMARY_POLYGLOT
    } else {
        CODING_AGENT_SUMMARY
    };
    let body = format_named(
        PROBLEM_DESCRIPTION_PROMPT,
        &[
            (
                "implementation_suggestion",
                &field("implementation_suggestion")?,
            ),
            ("problem_description", &field("problem_description")?),
        ],
    );
    Ok(format!("{summary}{body}"))
}

pub fn read_mdlog_file(path: &Path, filter: bool) -> io::Result<String> {
    if !filter {
        return read_file(path);
    }
    let text = read_text(path)?;
    let lines: Vec<&str> = text
        .split_inclusive('\n')
        .filter(|l| !l.starts_with(FILTERED_LOG_PREFIX))
        .map(|l| l.strip_suffix('\n').unwrap_or(l))
        .collect();
    Ok(lines.join("\n").trim().to_owned())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvalLogs {
    pub md_logs: Vec<String>,
    pub eval_logs: Vec<String>,
    pub predicted_patches: Vec<String>,
    pub eval_results: Vec<String>,
}

fn sorted_dirs(dir: &Path) -> io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

pub fn find_selfimprove_eval_logs(
    entry: &str,
    out_dir: &Path,
    commit_id: &str,
    filter: bool,
) -> io::Result<EvalLogs> {
    let commit_dir = out_dir.join(commit_id);
    let predictions = commit_dir.join("predictions");
    let folders = sorted_dirs(&predictions)?;
    let mut logs = EvalLogs::default();
    for f in &folders {
        let md = predictions.join(f).join(format!("{entry}.md"));
        if md.exists() {
            logs.md_logs.push(read_mdlog_file(&md, filter)?);
        }
    }
    for f in &folders {
        let json = predictions.join(f).join(format!("{entry}.json"));
        if json.exists() {
            let data: Value = serde_json::from_str(&read_text(&json)?)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            logs.predicted_patches
                .push(data.get("model_patch").map(value_text).unwrap_or_default());
            logs.eval_results
                .push(data.get("eval_result").map(value_text).unwrap_or_default());
        }
    }
    let tried: Vec<PathBuf> = folders
        .iter()
        .map(|f| predictions.join(f).join(format!("{entry}_eval.md")))
        .filter(|p| p.exists())
        .collect();
    if tried.is_empty() {
        for f in &folders {
            let report = commit_dir
                .join("logs/run_evaluation")
                .join(f)
                .join(f)
                .join(entry)
                .join("report.json");
            if report.exists() {
                let eval_json: Value = serde_json::from_str(&read_text(&report)?)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                logs.eval_logs.push(get_eval_log_text(&eval_json));
            }
        }
    } else {
        for file in tried {
            logs.eval_logs.push(read_file(&file)?);
        }
    }
    Ok(logs)
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalLog {
    pub md_log: String,
    pub eval_log: String,
    pub predicted_patch: String,
    pub eval_result: String,
}

pub fn process_selfimprove_eval_logs(logs: &EvalLogs) -> EvalLog {
    let first = |v: &[String], default: &str| v.first().cloned().unwrap_or_else(|| default.into());
    let mut md_log = first(&logs.md_logs, NO_LOGS);
    if md_log.chars().count() > MD_LOG_LIMIT {
        md_log = md_log.chars().take(MD_LOG_LIMIT).collect::<String>() + "\n<log clipped>";
    }
    EvalLog {
        md_log,
        eval_log: first(&logs.eval_logs, NO_TEST_RESULTS),
        predicted_patch: first(&logs.predicted_patches, NO_PATCH),
        eval_result: first(&logs.eval_results, NO_EVAL_RESULT),
    }
}

fn py_items(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().map(value_text).collect(),
        Some(Value::String(s)) => s.chars().map(String::from).collect(),
        Some(Value::Object(o)) => o.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

pub fn get_eval_log_text(eval_json: &Value) -> String {
    let status = eval_json
        .as_object()
        .and_then(|o| o.values().next())
        .and_then(|first| first.get("tests_status"))
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let empty = Value::Object(Default::default());
    let mut parts: Vec<String> = vec![
        "## New tests for the issue".into(),
        "These test whether the coding agent fixed the requested issue.".into(),
    ];
    let f2p = status.get("FAIL_TO_PASS").unwrap_or(&empty);
    let success = py_items(f2p.get("success"));
    if !success.is_empty() {
        parts.push(format!("Successfully fixed {}:", success.len()));
        parts.extend(success.iter().map(|t| format!("  \u{2713} {t}")));
    }
    let failure = py_items(f2p.get("failure"));
    if !failure.is_empty() {
        parts.push(format!("Failed to fix {} tests:", failure.len()));
        parts.extend(failure.iter().map(|t| format!("  \u{2717} {t}")));
    } else {
        parts.push("Pass All New Tests!".into());
    }
    parts.push("## Previous tests from the repo".into());
    parts.push(
        "These test whether the modification that coding agent made break the previous tests"
            .into(),
    );
    let p2p = status.get("PASS_TO_PASS").unwrap_or(&empty);
    let maintained = py_items(p2p.get("success"));
    if !maintained.is_empty() {
        parts.push(format!("\nMaintained {} passing tests", maintained.len()));
    }
    let failure = py_items(p2p.get("failure"));
    if !failure.is_empty() {
        parts.push(format!(
            "Regression in {} previously passing tests:",
            failure.len()
        ));
        parts.extend(failure.iter().map(|t| format!("  \u{2717} {t}")));
    } else {
        parts.push("Pass All Previous Tests!".into());
    }
    parts.join("\n")
}

pub fn relpath(path: &Path, start: &Path) -> String {
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (path, start) = (abs(path), abs(start));
    let normal = |p: &Path| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in p.components() {
            match c {
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(s) => out.push(s.to_string_lossy().into_owned()),
                other => out.push(other.as_os_str().to_string_lossy().into_owned()),
            }
        }
        out
    };
    let (p, s) = (normal(&path), normal(&start));
    let common = p.iter().zip(&s).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); s.len() - common];
    parts.extend(p[common..].iter().cloned());
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn walk_rust(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    files.sort();
    dirs.sort();
    out.extend(
        files
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "rs")),
    );
    for d in dirs {
        walk_rust(&d, out);
    }
}

fn code_file_text(
    current_dir: &Path,
    file: &str,
    excluded: &dyn Fn(&str) -> bool,
) -> io::Result<Vec<String>> {
    let full = current_dir.join(file);
    let mut files = Vec::new();
    if full.is_file() {
        files.push((relpath(&full, current_dir), full.clone()));
    } else if full.is_dir() {
        let mut found = Vec::new();
        walk_rust(&full, &mut found);
        files.extend(found.into_iter().map(|f| (relpath(&f, current_dir), f)));
    }
    let mut code = Vec::new();
    for (rel, path) in files.into_iter().filter(|(rel, _)| !excluded(rel)) {
        code.push(format!("# {rel}"));
        code.push(read_file(&path)?);
    }
    Ok(code)
}

pub fn get_current_code<S: AsRef<str>>(
    current_dir: &Path,
    code_files: &[S],
    patch_files: &[PathBuf],
    exclude_files: &[S],
) -> io::Result<String> {
    let excluded = |p: &str| exclude_files.iter().any(|e| e.as_ref() == p);
    let mut code = Vec::new();
    for file in code_files
        .iter()
        .map(AsRef::as_ref)
        .filter(|f| !excluded(f))
    {
        code.extend(code_file_text(current_dir, file, &excluded)?);
    }
    for (i, patch) in patch_files.iter().enumerate() {
        let rel = relpath(patch, current_dir);
        if !excluded(&rel) {
            code.push(format!("# Patch {}: {rel}", i + 1));
            code.push(read_file(patch)?);
        }
    }
    Ok(code.join("\n"))
}

pub fn find_entry<'a>(dataset: &'a [Value], instance_id: &str) -> Option<&'a Value> {
    dataset
        .iter()
        .find(|e| e.get("instance_id").and_then(Value::as_str) == Some(instance_id))
}

fn entry_field(entry: &Value, key: &str) -> Result<String, String> {
    entry
        .get(key)
        .map(value_text)
        .ok_or_else(|| format!("dataset entry has no {key}"))
}

fn issue_prompt(issue: &str, log: &EvalLog, test_patch: &str, github_issue: &str) -> String {
    let body = format_named(
        DIAGNOSE_PROMPT,
        &[
            ("md_log", &log.md_log),
            ("eval_log", &log.eval_log),
            ("predicted_patch", &log.predicted_patch),
            ("test_patch", test_patch),
            ("github_issue", github_issue),
        ],
    );
    format!("{issue}{body}")
}

pub struct DiagnoseInput<'a> {
    pub entry_id: &'a str,
    pub commit: &'a str,
    pub root_dir: &'a Path,
    pub out_dir: &'a Path,
    pub dataset: &'a [Value],
    pub patch_files: &'a [PathBuf],
}

pub fn get_diagnose_prompt_swe(input: &DiagnoseInput) -> Result<(String, String), String> {
    let prompt = match input.entry_id {
        SOLVE_EMPTY_PATCHES => DIAGNOSE_PROMPT_EMPTYPATCHES.to_owned(),
        SOLVE_STOCHASTICITY => DIAGNOSE_PROMPT_STOCHASTICITY.to_owned(),
        SOLVE_CONTEXTLENGTH => DIAGNOSE_PROMPT_CONTEXTLENGTH.to_owned(),
        id => {
            let logs = find_selfimprove_eval_logs(id, input.out_dir, input.commit, true)
                .map_err(|e| e.to_string())?;
            let log = process_selfimprove_eval_logs(&logs);
            let entry = find_entry(input.dataset, id)
                .ok_or_else(|| format!("Could not find entry with id {id} in dataset."))?;
            issue_prompt(
                SWE_ISSUE_PROMPT,
                &log,
                &entry_field(entry, "test_patch")?,
                &entry_field(entry, "problem_statement")?,
            )
        }
    };
    let code = get_current_code(input.root_dir, &CODE_FILES, input.patch_files, &[])
        .map_err(|e| e.to_string())?;
    let system = format!(
        "{CODING_AGENT_SUMMARY}{}",
        format_named(DIAGNOSE_SYSTEM_MESSAGE, &[("code", &code)])
    );
    Ok((system, prompt))
}

pub fn get_diagnose_prompt_polyglot<R: Rng + ?Sized>(
    input: &DiagnoseInput,
    rng: &mut R,
) -> Result<(String, String), String> {
    let logs = find_selfimprove_eval_logs(input.entry_id, input.out_dir, input.commit, true)
        .map_err(|e| e.to_string())?;
    let log = process_selfimprove_eval_logs(&logs);
    let entry = find_entry(input.dataset, input.entry_id).ok_or_else(|| {
        format!(
            "Could not find entry with id {} in dataset.",
            input.entry_id
        )
    })?;
    let is_polyglot = entry.get("language").is_some();
    let test_patch = entry_field(
        entry,
        if is_polyglot {
            "reference_tests"
        } else {
            "test_patch"
        },
    )?;
    let github_issue = entry_field(entry, "problem_statement")?;
    let code = get_current_code(input.root_dir, &CODE_FILES, input.patch_files, &[])
        .map_err(|e| e.to_string())?;
    let system = format!(
        "{CODING_AGENT_SUMMARY_POLYGLOT}{}",
        format_named(DIAGNOSE_SYSTEM_MESSAGE, &[("code", &code)])
    );
    if rng.random::<f64>() < 0.25 {
        let prompt = format_named(
            DIAGNOSE_PROMPT_STOCHASTICITY_POLYGLOT,
            &[("md_log", &log.md_log)],
        );
        return Ok((system, prompt));
    }
    if log.eval_result.contains("empty_patch") {
        let prompt = format_named(
            DIAGNOSE_PROMPT_EMPTYPATCHES_POLYGLOT,
            &[("md_log", &log.md_log)],
        );
        return Ok((system, prompt));
    }
    Ok((
        system,
        issue_prompt(POLYGLOT_ISSUE_PROMPT, &log, &test_patch, &github_issue),
    ))
}

fn context_length_pattern() -> Regex {
    let alternatives: Vec<String> = CONTEXT_LENGTH_ERRORS
        .iter()
        .map(|e| {
            let e = e.replace('{', r"\{").replace('}', r"\}");
            format!("{e}\n{e}")
        })
        .collect();
    Regex::new(&alternatives.join("|")).unwrap()
}

pub fn any_exceeding_context_length<S: AsRef<str>>(
    out_dir: &Path,
    commit_id: &str,
    instance_ids: &[S],
) -> io::Result<bool> {
    let pattern = context_length_pattern();
    for id in instance_ids {
        let logs = find_selfimprove_eval_logs(id.as_ref(), out_dir, commit_id, false)?;
        if logs.md_logs.iter().any(|log| pattern.is_match(log)) {
            return Ok(true);
        }
    }
    Ok(false)
}
