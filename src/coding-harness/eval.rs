use std::path::Path;

use rand::seq::IndexedRandom;
use serde_json::Value;

use crate::llm::{Client, extract_json_between_markers, get_response_from_llm};
use crate::parsers::{Report, parser_for_repo};
use crate::withtools::convert_msg_history;

pub fn get_acc_on_tasks(tasks: &[String], commit_path: &Path) -> std::io::Result<Vec<u8>> {
    if tasks.is_empty() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(commit_path.join("metadata.json"))?;
    let metadata: Value = serde_json::from_str(&text)?;
    let resolved = &metadata["overall_performance"]["total_resolved_ids"];
    Ok(tasks
        .iter()
        .map(|t| {
            u8::from(
                resolved
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == t.as_str())),
            )
        })
        .collect())
}

pub fn repo_of(instance_id: &str) -> String {
    if instance_id == "rustyhgm" {
        return "rustyhgm".to_owned();
    }
    let replaced = instance_id.replace("__", "/");
    let parts: Vec<&str> = replaced.split('-').collect();
    parts[..parts.len() - 1].join("-")
}

pub fn parse_eval_output(instance_id: &str, eval_output: &str) -> Report {
    parser_for_repo(&repo_of(instance_id))
        .and_then(|parse| parse(eval_output))
        .unwrap_or_default()
}

pub fn msg_history_to_report(instance_id: &str, history: &[Value], model: &str) -> Report {
    convert_msg_history(history, model)
        .iter()
        .rev()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .filter(|c| c.contains("Tool Result:"))
        .map(|c| parse_eval_output(instance_id, c))
        .find(|r| !r.is_empty())
        .unwrap_or_default()
}

pub fn get_report_score(report: &Report) -> f64 {
    if report.is_empty() {
        return 0.0;
    }
    let passed = report.values().filter(|v| *v == "PASSED").count();
    passed as f64 / report.len() as f64
}

pub const TIE_BREAKER_SYSTEM: &str = "You are an excellent software engineer who has been asked to evaluate the proposed solutions to a problem statement.";

pub fn tie_breaker_prompt(
    problem_statement: &str,
    candidates: &[(String, String)],
    indices: &[usize],
) -> String {
    let solutions: Vec<String> = indices
        .iter()
        .enumerate()
        .map(|(i, &index)| {
            let n = i + 1;
            format!(
                "# Proposed solution {n}\n\n<code_diff_{n}>\n{}\n</code_diff{n}>\n<test_report_{n}>\n{}\n</test_report_{n}>",
                candidates[index].0, candidates[index].1
            )
        })
        .collect();
    format!(
        "Given the following problem statement, proposed solutions, and test reports, provide a summary of the differences between the code diffs and an evaluation of the proposed solutions.\n\n<problem_description>\n{problem_statement}\n</problem_description>\n\n{}\n\nRespond precisely in the following format including the JSON start and end markers:\n\n```json\n<JSON>\n```\n\nIn <JSON>, provide a JSON response with the following fields:\n- \"difference_summary\": Summary of the differences between the code diffs.\n- \"reasoning\": Explanation of the reasoning behind the evaluation.\n- \"scores\": List of numerical scores for each proposed solution.\n\nYour response will be automatically parsed, so ensure that the string response is precisely in the correct format. Do NOT include `<JSON>` tag in your output.\n",
        solutions.join("\n\n")
    )
}

fn llm_choice(client: &Client, prompt: &str, log: &dyn Fn(&str)) -> Result<usize, String> {
    let (response, _) = get_response_from_llm(prompt, client, TIE_BREAKER_SYSTEM, &[], 0.7)
        .map_err(|e| e.to_string())?;
    log(&format!("{response:?}"));
    let scores: Vec<f64> = extract_json_between_markers(&response)
        .and_then(|j| j["scores"].as_array().cloned())
        .ok_or_else(|| format!("no scores in {response:?}"))?
        .iter()
        .map(|s| {
            s.as_f64()
                .ok_or_else(|| format!("score {s} is not a number"))
        })
        .collect::<Result<_, _>>()?;
    let best = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let ties: Vec<usize> = (0..scores.len()).filter(|&i| scores[i] == best).collect();
    ties.choose(&mut rand::rng())
        .copied()
        .ok_or_else(|| "no scores".to_owned())
}

pub fn score_tie_breaker(
    client: &Client,
    problem_statement: &str,
    candidates: &[(String, String)],
    best_score_indices: &[usize],
    log: &dyn Fn(&str),
) -> usize {
    let indices: Vec<usize> = if best_score_indices.is_empty() {
        (0..candidates.len()).collect()
    } else {
        best_score_indices.to_vec()
    };
    let prompt = tie_breaker_prompt(problem_statement, candidates, &indices);
    match llm_choice(client, &prompt, log).and_then(|i| {
        indices
            .get(i)
            .copied()
            .ok_or_else(|| format!("index {i} out of range"))
    }) {
        Ok(index) => index,
        Err(e) => {
            log(&format!("Error in score_tie_breaker: {e}"));
            indices[0]
        }
    }
}
