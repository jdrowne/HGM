use std::cell::RefCell;
use std::fs;

use hgm_agent::eval::{
    TIE_BREAKER_SYSTEM, get_acc_on_tasks, get_report_score, msg_history_to_report,
    parse_eval_output, repo_of, score_tie_breaker, tie_breaker_prompt,
};
use hgm_agent::llm::{Client, Provider};
use serde_json::{Value, json};

use crate::common::{cases, content, ok, refused, serve};

#[test]
fn eval_helpers_match_the_python() {
    for c in cases("eval_utils") {
        let name = c["name"].as_str().unwrap();
        let input = &c["input"];
        if name.starts_with("parse_eval_output_") {
            let got = parse_eval_output(
                input["instance_id"].as_str().unwrap(),
                input["eval_output"].as_str().unwrap(),
            );
            assert_eq!(Value::Object(got), c["output"], "{name}");
        } else if name.starts_with("get_report_score_") {
            let got = get_report_score(input.as_object().unwrap());
            assert_eq!(got, c["output"].as_f64().unwrap(), "{name}");
        } else {
            let got = msg_history_to_report(
                input["instance_id"].as_str().unwrap(),
                input["msg_history"].as_array().unwrap(),
                input["model"].as_str().unwrap(),
            );
            assert_eq!(Value::Object(got), c["output"], "{name}");
        }
    }
    assert_eq!(repo_of("hgm"), "hgm");
    assert_eq!(repo_of("pvlib__pvlib-python-1072"), "pvlib/pvlib-python");
}

#[test]
fn accuracy_follows_the_resolved_ids() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(get_acc_on_tasks(&[], tmp.path()).unwrap(), Vec::<u8>::new());
    let tasks = ["a".to_owned(), "b".to_owned()];
    assert!(get_acc_on_tasks(&tasks, tmp.path()).is_err());
    fs::write(tmp.path().join("metadata.json"), "{").unwrap();
    assert!(get_acc_on_tasks(&tasks, tmp.path()).is_err());
    let metadata = json!({"overall_performance": {"total_resolved_ids": ["b"]}});
    fs::write(tmp.path().join("metadata.json"), metadata.to_string()).unwrap();
    assert_eq!(get_acc_on_tasks(&tasks, tmp.path()).unwrap(), [0, 1]);
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
    assert!(prompt.ends_with("Do NOT include `<JSON>` tag in your output.\n"));
}

#[test]
fn the_tie_breaker_picks_a_top_score_or_falls_back() {
    let all = candidates(3);
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let (base, seen) = serve(vec![content("```json\n{\"scores\": [1, 5]}\n```")]);
    assert_eq!(
        score_tie_breaker(&client(&base), "p", &all, &[0, 2], &log),
        2
    );
    let sent: Value = serde_json::from_str(&seen.lock().unwrap()[0].body).unwrap();
    assert!(
        sent["messages"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with(TIE_BREAKER_SYSTEM)
    );
    let (base, _) = serve(vec![content("```json\n{\"scores\": [3, 3, 3]}\n```")]);
    assert!(score_tie_breaker(&client(&base), "p", &all, &[], &log) < 3);
    let fallbacks = [
        content("no json"),
        content("```json\n{\"scores\": [\"x\"]}\n```"),
        content("```json\n{\"scores\": []}\n```"),
        content("```json\n{\"scores\": [1, 2, 9]}\n```"),
        ok(json!({})),
    ];
    let (base, _) = serve(fallbacks.to_vec());
    for _ in 0..fallbacks.len() {
        assert_eq!(
            score_tie_breaker(&client(&base), "p", &all, &[1, 2], &log),
            1
        );
    }
    assert_eq!(
        score_tie_breaker(&client(&refused()), "p", &all, &[2], &log),
        2
    );
    let errors = lines
        .borrow()
        .iter()
        .filter(|l| l.starts_with("Error in score_tie_breaker: "))
        .count();
    assert_eq!(errors, 6);
}
