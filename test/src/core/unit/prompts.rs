use std::fs;
use std::path::PathBuf;

use hgm_core::prompts::{
    self, EvalLogs, any_exceeding_context_length, format_named, get_eval_log_text,
    get_problem_description_prompt, process_selfimprove_eval_logs, read_mdlog_file, relpath,
};
use serde_json::{Value, json};

use crate::common::{cases, fixtures_dir};

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn templates_match_the_python() {
    let dir = fixtures_dir().join("prompts");
    let templates = [
        ("coding_agent_summary", prompts::CODING_AGENT_SUMMARY),
        (
            "coding_agent_summary_polyglot",
            prompts::CODING_AGENT_SUMMARY_POLYGLOT,
        ),
        ("diagnose_system_message", prompts::DIAGNOSE_SYSTEM_MESSAGE),
        ("swe_issue_prompt", prompts::SWE_ISSUE_PROMPT),
        ("polyglot_issue_prompt", prompts::POLYGLOT_ISSUE_PROMPT),
        ("diagnose_prompt", prompts::DIAGNOSE_PROMPT),
        (
            "diagnose_prompt_emptypatches",
            prompts::DIAGNOSE_PROMPT_EMPTYPATCHES,
        ),
        (
            "diagnose_prompt_stochasticity",
            prompts::DIAGNOSE_PROMPT_STOCHASTICITY,
        ),
        (
            "diagnose_prompt_contextlength",
            prompts::DIAGNOSE_PROMPT_CONTEXTLENGTH,
        ),
        (
            "problem_description_prompt",
            prompts::PROBLEM_DESCRIPTION_PROMPT,
        ),
        (
            "diagnose_prompt_emptypatches_polyglot",
            prompts::DIAGNOSE_PROMPT_EMPTYPATCHES_POLYGLOT,
        ),
        (
            "diagnose_prompt_stochasticity_polyglot",
            prompts::DIAGNOSE_PROMPT_STOCHASTICITY_POLYGLOT,
        ),
    ];
    let on_disk = fs::read_dir(&dir).unwrap().count();
    assert_eq!(on_disk, templates.len());
    for (name, text) in templates {
        let expected = fs::read_to_string(dir.join(format!("{name}.txt"))).unwrap();
        assert_eq!(text, expected, "{name}");
    }
}

#[test]
fn helpers_match_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    for case in cases("self_improvement_prompt") {
        let name = case["name"].as_str().unwrap();
        let input = &case["input"];
        let output = &case["output"];
        if name.starts_with("get_problem_description_prompt") {
            let got = get_problem_description_prompt(
                &input["response_json"],
                input["is_polyglot"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(got, output.as_str().unwrap(), "{name}");
        } else if name.starts_with("process_selfimprove_eval_logs") {
            let args: Vec<Vec<String>> = input.as_array().unwrap().iter().map(strings).collect();
            let mut logs = EvalLogs {
                md_logs: args[0].clone(),
                eval_logs: args[1].clone(),
                predicted_patches: args[2].clone(),
                eval_results: args[3].clone(),
            };
            if name.ends_with("clipped") {
                logs.md_logs = vec!["x".repeat(100_005)];
            }
            let got = process_selfimprove_eval_logs(&logs);
            let shown = if name.ends_with("clipped") {
                let tail: String = got
                    .md_log
                    .chars()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                json!({"len": got.md_log.chars().count(), "tail": tail})
            } else {
                json!(got.md_log)
            };
            assert_eq!(
                json!([shown, got.eval_log, got.predicted_patch, got.eval_result]),
                *output,
                "{name}"
            );
        } else if name.starts_with("get_eval_log_text") {
            assert_eq!(get_eval_log_text(input), output.as_str().unwrap(), "{name}");
        } else if name.starts_with("read_mdlog_file") {
            let path = tmp.path().join("log.md");
            fs::write(&path, input["text"].as_str().unwrap()).unwrap();
            let got = read_mdlog_file(&path, input["filter"].as_bool().unwrap()).unwrap();
            assert_eq!(got, output.as_str().unwrap(), "{name}");
        } else {
            panic!("unhandled case {name}");
        }
    }
}

#[test]
fn context_length_matches_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    for case in cases("context_length") {
        let name = case["name"].as_str().unwrap();
        let pred = tmp.path().join(name).join("predictions").join("run0");
        fs::create_dir_all(&pred).unwrap();
        fs::write(
            pred.join("inst-1.md"),
            case["input"]["md_log"].as_str().unwrap(),
        )
        .unwrap();
        let got = any_exceeding_context_length(tmp.path(), name, &["inst-1"]).unwrap();
        assert_eq!(json!(got), case["output"], "{name}");
    }
}

#[test]
fn format_named_substitutes_once() {
    assert_eq!(
        format_named("{a} and {b} {c}", &[("a", "{b}"), ("b", "B")]),
        "{b} and B {c}"
    );
}

#[test]
fn relpath_walks_up_and_down() {
    let root = PathBuf::from("/x/y");
    assert_eq!(relpath(&root.join("a/b.py"), &root), "a/b.py");
    assert_eq!(relpath(&PathBuf::from("/x/z/p.diff"), &root), "../z/p.diff");
    assert_eq!(relpath(&root, &root), ".");
}
