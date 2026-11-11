use std::cell::RefCell;
use std::fs;

use rustyhgm_coding_agent::eval::{
    TIE_BREAKER_SYSTEM, get_acc_on_tasks, get_report_score, msg_history_to_report,
    parse_eval_output, repo_of, score_tie_breaker, tie_breaker_prompt,
};
use rustyhgm_coding_agent::llm::{Client, Provider};
use serde_json::{Value, json};

use crate::common::{Seen, cases, content, ok, refused, serve};

fn eval_cases(prefix: &str) -> Vec<Value> {
    cases("eval_utils")
        .into_iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with(prefix))
        .collect()
}

fn named(c: &Value, value: Value) -> (String, Value) {
    (c["name"].as_str().unwrap().to_owned(), value)
}

fn expected(fixtures: &[Value]) -> Vec<(String, Value)> {
    fixtures
        .iter()
        .map(|c| named(c, c["output"].clone()))
        .collect()
}

#[test]
fn eval_outputs_parse_like_the_python() {
    let fixtures = eval_cases("parse_eval_output_");
    let got: Vec<(String, Value)> = fixtures
        .iter()
        .map(|c| {
            let input = &c["input"];
            let report = parse_eval_output(
                input["instance_id"].as_str().unwrap(),
                input["eval_output"].as_str().unwrap(),
            );
            named(c, Value::Object(report))
        })
        .collect();
    assert_eq!(got, expected(&fixtures));
}

#[test]
fn report_scores_match_the_python() {
    let fixtures = eval_cases("get_report_score_");
    let got: Vec<(String, f64)> = fixtures
        .iter()
        .map(|c| {
            let name = c["name"].as_str().unwrap().to_owned();
            (name, get_report_score(c["input"].as_object().unwrap()))
        })
        .collect();
    let want: Vec<(String, f64)> = fixtures
        .iter()
        .map(|c| {
            let name = c["name"].as_str().unwrap().to_owned();
            (name, c["output"].as_f64().unwrap())
        })
        .collect();
    assert_eq!(got, want);
}

#[test]
fn message_histories_become_reports_like_the_python() {
    let fixtures = eval_cases("msg_history_to_report_");
    let got: Vec<(String, Value)> = fixtures
        .iter()
        .map(|c| {
            let input = &c["input"];
            let report = msg_history_to_report(
                input["instance_id"].as_str().unwrap(),
                input["msg_history"].as_array().unwrap(),
                input["model"].as_str().unwrap(),
            );
            named(c, Value::Object(report))
        })
        .collect();
    assert_eq!(got, expected(&fixtures));
}

#[test]
fn every_eval_fixture_belongs_to_a_helper() {
    let all = cases("eval_utils").len();
    let known = [
        "parse_eval_output_",
        "get_report_score_",
        "msg_history_to_report_",
    ]
    .iter()
    .map(|p| eval_cases(p).len())
    .sum::<usize>();
    assert_eq!(known, all);
}

#[test]
fn an_instance_id_without_separators_is_its_own_repo() {
    assert_eq!(repo_of("rustyhgm"), "rustyhgm");
}

#[test]
fn an_swe_instance_id_names_its_repo() {
    assert_eq!(repo_of("pvlib__pvlib-python-1072"), "pvlib/pvlib-python");
}

fn tasks() -> [String; 2] {
    ["a".to_owned(), "b".to_owned()]
}

#[test]
fn no_tasks_have_no_accuracy() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(get_acc_on_tasks(&[], tmp.path()).unwrap(), Vec::<u8>::new());
}

#[test]
fn missing_metadata_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(get_acc_on_tasks(&tasks(), tmp.path()).is_err());
}

#[test]
fn malformed_metadata_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("metadata.json"), "{").unwrap();
    assert!(get_acc_on_tasks(&tasks(), tmp.path()).is_err());
}

#[test]
fn accuracy_follows_the_resolved_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let metadata = json!({"overall_performance": {"total_resolved_ids": ["b"]}});
    fs::write(tmp.path().join("metadata.json"), metadata.to_string()).unwrap();
    assert_eq!(get_acc_on_tasks(&tasks(), tmp.path()).unwrap(), [0, 1]);
}

fn client(base: &str) -> Client {
    Client {
        provider: Provider::OpenAi,
        model: "o3".to_owned(),
        base_url: base.to_owned(),
        api_key: None,
    }
}

fn candidates(n: usize) -> Vec<(String, String)> {
    (0..n).map(|i| (format!("d{i}"), format!("r{i}"))).collect()
}

#[test]
fn the_tie_breaker_prompt_lists_each_candidate() {
    let prompt = tie_breaker_prompt("p", &candidates(2), &[1]);
    assert!(prompt.contains("<problem_description>\np\n</problem_description>\n\n# Proposed solution 1\n\n<code_diff_1>\nd1\n</code_diff1>\n<test_report_1>\nr1\n</test_report_1>\n\nRespond"));
}

#[test]
fn the_tie_breaker_prompt_ends_with_the_format_rule() {
    let prompt = tie_breaker_prompt("p", &candidates(2), &[1]);
    assert!(prompt.ends_with("Do NOT include `<JSON>` tag in your output.\n"));
}

fn tie_break(base: &str, among: &[usize]) -> (usize, Vec<String>) {
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let pick = score_tie_breaker(&client(base), "p", &candidates(3), among, &log);
    (pick, lines.into_inner())
}

fn top_score() -> (usize, Seen) {
    let (base, seen) = serve(vec![content("```json\n{\"scores\": [1, 5]}\n```")]);
    (tie_break(&base, &[0, 2]).0, seen)
}

#[test]
fn the_tie_breaker_picks_the_top_score() {
    assert_eq!(top_score().0, 2);
}

#[test]
fn the_tie_breaker_sends_its_system_prompt() {
    let (_, seen) = top_score();
    let sent: Value = serde_json::from_str(&seen.lock().unwrap()[0].body).unwrap();
    assert!(
        sent["messages"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with(TIE_BREAKER_SYSTEM)
    );
}

#[test]
fn the_tie_breaker_picks_among_every_candidate_when_none_lead() {
    let (base, _) = serve(vec![content("```json\n{\"scores\": [3, 3, 3]}\n```")]);
    assert!(tie_break(&base, &[]).0 < 3);
}

fn fallback(response: (u16, Vec<u8>)) -> (usize, Vec<String>) {
    let (base, _) = serve(vec![response]);
    tie_break(&base, &[1, 2])
}

fn fallbacks() -> Vec<(u16, Vec<u8>)> {
    vec![
        content("no json"),
        content("```json\n{\"scores\": [\"x\"]}\n```"),
        content("```json\n{\"scores\": []}\n```"),
        content("```json\n{\"scores\": [1, 2, 9]}\n```"),
        ok(json!({})),
    ]
}

#[test]
fn the_tie_breaker_falls_back_without_json() {
    assert_eq!(fallback(fallbacks().remove(0)).0, 1);
}

#[test]
fn the_tie_breaker_falls_back_on_a_score_that_is_not_a_number() {
    assert_eq!(fallback(fallbacks().remove(1)).0, 1);
}

#[test]
fn the_tie_breaker_falls_back_without_scores() {
    assert_eq!(fallback(fallbacks().remove(2)).0, 1);
}

#[test]
fn the_tie_breaker_falls_back_when_the_best_score_is_out_of_range() {
    assert_eq!(fallback(fallbacks().remove(3)).0, 1);
}

#[test]
fn the_tie_breaker_falls_back_without_a_reply() {
    assert_eq!(fallback(fallbacks().remove(4)).0, 1);
}

#[test]
fn the_tie_breaker_falls_back_when_the_server_is_down() {
    assert_eq!(tie_break(&refused(), &[2]).0, 2);
}

#[test]
fn every_tie_breaker_fallback_logs_one_error() {
    let mut logs: Vec<Vec<String>> = fallbacks().into_iter().map(|r| fallback(r).1).collect();
    logs.push(tie_break(&refused(), &[2]).1);
    let errors: Vec<usize> = logs
        .iter()
        .map(|lines| {
            lines
                .iter()
                .filter(|l| l.starts_with("Error in score_tie_breaker: "))
                .count()
        })
        .collect();
    assert_eq!(errors, [1; 6]);
}
