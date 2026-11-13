use std::cell::RefCell;
use std::fs;
use std::time::Duration;

use rustyhgm_coding_harness::llm::{Client, MAX_OUTPUT_TOKENS, Provider};
use rustyhgm_coding_harness::tools::tool_infos;
use rustyhgm_coding_harness::withtools::{
    Backoff, CUT_OFF, Chat, SYSTEM_MESSAGE, ToolUse, check_for_tool_use, convert_msg_history,
    convert_tool_info, is_cut_off, is_openai, request_limit,
};
use serde_json::{Value, json};

use crate::common::{Seen, cases, ok, refused, serve, silent};

#[test]
fn tool_conversion_matches_the_python() {
    let mismatches: Vec<String> = cases("tool_conversion")
        .iter()
        .filter_map(|c| {
            let input = &c["input"];
            let got = convert_tool_info(&input["tool_info"], input["model"].as_str().unwrap());
            (got != c["output"]).then(|| format!("{}: {got}", c["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn odd_property_types_convert_like_the_python() {
    let odd = json!({"name": "n", "description": "d", "input_schema": {"properties": {"a": {"type": 1}, "b": {}}}});
    assert_eq!(
        convert_tool_info(&odd, "gpt-5")["parameters"],
        json!({"properties": {"a": {"type": 1}, "b": {"type": null}}, "additionalProperties": false, "required": ["a", "b"]})
    );
}

fn bare() -> Value {
    json!({"name": "n", "description": "d", "input_schema": {}})
}

#[test]
fn a_bare_schema_has_no_openai_parameters() {
    assert_eq!(convert_tool_info(&bare(), "gpt-5")["parameters"], json!({}));
}

#[test]
fn a_bare_schema_requires_nothing_elsewhere() {
    assert_eq!(
        convert_tool_info(&bare(), "qwen")["function"]["parameters"]["required"],
        json!([])
    );
}

#[test]
fn o_models_are_openai() {
    assert!(is_openai("o3"));
}

#[test]
fn gpt_models_are_openai_in_any_case() {
    assert!(is_openai("my-GPT"));
}

#[test]
fn other_models_are_not_openai() {
    assert!(!is_openai("qwen"));
}

#[test]
fn message_histories_convert_like_the_python() {
    let mismatches: Vec<String> = cases("msg_history")
        .iter()
        .filter_map(|c| {
            let input = &c["input"];
            let history = input["msg_history"].as_array().unwrap();
            let got = Value::Array(convert_msg_history(
                history,
                input["model"].as_str().unwrap(),
            ));
            (got != c["output"]).then(|| format!("{}: {got}", c["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn odd_openai_history_items_become_messages() {
    let items = [
        json!({"type": "function_call", "name": "bash"}),
        json!({"content": "x"}),
        json!(3),
    ];
    assert_eq!(
        convert_msg_history(&items, "gpt-5"),
        [
            json!({"role": "assistant", "content": "{\"type\":\"function_call\",\"name\":\"bash\"}"}),
            json!({"role": "user", "content": "x"}),
            json!({"role": "assistant", "content": "3"}),
        ]
    );
}

#[test]
fn tool_calls_in_history_become_text_for_other_models() {
    let calls = [
        json!({"role": "assistant", "tool_calls": [{"function": {"name": "editor", "arguments": "{}"}}]}),
        json!({"role": "tool", "content": 1}),
    ];
    assert_eq!(
        convert_msg_history(&calls, "qwen"),
        [
            json!({"role": "assistant", "content": "Function: editor\nArguments: {}"}),
            json!({"role": "tool", "content": 1}),
        ]
    );
}

fn listing() -> ToolUse {
    ToolUse {
        id: "i".to_owned(),
        name: "bash".to_owned(),
        input: json!({"command": "ls"}),
    }
}

#[test]
fn a_tool_call_is_found_in_a_chat_response() {
    let chat = json!({"choices": [{"message": {"tool_calls": [{"id": "i", "function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}]}}]});
    assert_eq!(check_for_tool_use(&chat, "qwen").unwrap(), Some(listing()));
}

#[test]
fn an_empty_tool_call_list_is_no_call() {
    let empty = json!({"choices": [{"message": {"tool_calls": []}}]});
    assert_eq!(check_for_tool_use(&empty, "qwen").unwrap(), None);
}

#[test]
fn a_tool_call_is_found_in_an_openai_response() {
    let openai = json!({"output": [{"type": "reasoning"}, {"type": "function_call", "call_id": "i", "name": "bash", "arguments": "{\"command\": \"ls\"}"}]});
    assert_eq!(
        check_for_tool_use(&openai, "gpt-5").unwrap(),
        Some(listing())
    );
}

#[test]
fn an_openai_text_reply_is_no_call() {
    let text = json!({"output": [{"type": "message", "role": "assistant", "content": []}]});
    assert_eq!(check_for_tool_use(&text, "gpt-5").unwrap(), None);
}

#[test]
fn broken_tool_arguments_are_an_error() {
    let broken = json!({"output": [{"type": "function_call", "arguments": "{"}]});
    assert!(check_for_tool_use(&broken, "o3").is_err());
}

fn client(provider: Provider, model: &str, base: &str) -> Client {
    Client {
        provider,
        model: model.to_owned(),
        base_url: base.to_owned(),
        api_key: None,
    }
}

fn body(seen: &Seen, i: usize) -> Value {
    serde_json::from_str(&seen.lock().unwrap()[i].body).unwrap()
}

fn tool_call(id: &str, name: &str, arguments: &Value) -> Value {
    json!({"choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [{"id": id, "type": "function", "function": {"name": name, "arguments": arguments.to_string()}}]}}]})
}

fn reply(text: &str) -> Value {
    json!({"choices": [{"message": {"role": "assistant", "content": text}}]})
}

struct Ran {
    history: Vec<Value>,
    calls: usize,
    lines: Vec<String>,
}

fn chat_on(client: Client, setup: impl FnOnce(&mut Chat), msg: &str, before: &[Value]) -> Ran {
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let (history, calls) = {
        let mut chat = Chat::new(client, &log);
        setup(&mut chat);
        chat.run(msg, before)
    };
    Ran {
        history,
        calls,
        lines: lines.into_inner(),
    }
}

fn served(
    responses: Vec<(u16, Vec<u8>)>,
    provider: Provider,
    model: &str,
    setup: impl FnOnce(&mut Chat),
) -> (Ran, Seen) {
    let (base, seen) = serve(responses);
    (
        chat_on(client(provider, model, &base), setup, "x", &[]),
        seen,
    )
}

struct ToolRun {
    _tmp: tempfile::TempDir,
    file: std::path::PathBuf,
    edit: Value,
    before: Value,
    ran: Ran,
    seen: Seen,
}

fn tool_run() -> ToolRun {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("f.txt");
    let edit = json!({"command": "create", "path": file, "file_text": "hi"});
    let (base, seen) = serve(vec![
        ok(json!({"data": [{"id": "served"}]})),
        ok(tool_call("c1", "editor", &edit)),
        ok(json!({"data": [{"id": "served"}]})),
        ok(reply("done")),
    ]);
    let before = json!({"role": "user", "content": "earlier"});
    let ran = chat_on(
        client(Provider::Vllm, "vllm-x", &base),
        |_| {},
        "do it",
        std::slice::from_ref(&before),
    );
    ToolRun {
        _tmp: tmp,
        file,
        edit,
        before,
        ran,
        seen,
    }
}

fn created(file: &std::path::Path) -> String {
    format!("File created successfully at: {}", file.display())
}

#[test]
fn a_chat_counts_every_llm_call() {
    assert_eq!(tool_run().ran.calls, 2);
}

#[test]
fn a_chat_runs_the_tools_the_model_calls() {
    let run = tool_run();
    assert_eq!(fs::read_to_string(&run.file).unwrap(), "hi");
}

#[test]
fn a_chat_records_the_whole_exchange() {
    let run = tool_run();
    assert_eq!(
        run.ran.history,
        [
            run.before.clone(),
            json!({"role": "user", "content": "do it"}),
            tool_call("c1", "editor", &run.edit)["choices"][0]["message"].clone(),
            json!({"role": "tool", "tool_call_id": "c1", "name": "editor", "content": created(&run.file)}),
            reply("done")["choices"][0]["message"].clone(),
        ]
    );
}

fn first_request() -> Value {
    body(&tool_run().seen, 1)
}

#[test]
fn a_chat_asks_for_the_served_model() {
    assert_eq!(first_request()["model"], "served");
}

#[test]
fn a_chat_lets_the_model_choose_tools() {
    assert_eq!(first_request()["tool_choice"], "auto");
}

#[test]
fn a_chat_asks_for_one_tool_call_at_a_time() {
    assert_eq!(first_request()["parallel_tool_calls"], false);
}

#[test]
fn a_chat_starts_with_the_system_message() {
    assert_eq!(
        first_request()["messages"][0],
        json!({"role": "system", "content": SYSTEM_MESSAGE})
    );
}

#[test]
fn a_chat_sends_the_system_history_and_instruction() {
    assert_eq!(first_request()["messages"].as_array().unwrap().len(), 3);
}

#[test]
fn a_chat_offers_the_converted_tools() {
    let tools: Vec<Value> = tool_infos()
        .iter()
        .map(|t| convert_tool_info(t, "vllm-x"))
        .collect();
    assert_eq!(first_request()["tools"], json!(tools));
}

#[test]
fn a_chat_sends_the_tool_result_back() {
    assert_eq!(
        body(&tool_run().seen, 3)["messages"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn a_chat_logs_the_instruction() {
    assert_eq!(
        tool_run().ran.lines[0],
        "\n========== User Instruction ==========\ndo it"
    );
}

#[test]
fn a_chat_logs_the_tool_response() {
    assert!(tool_run().ran.lines[1].starts_with("Tool Response: {"));
}

#[test]
fn a_chat_logs_the_tool_used() {
    assert_eq!(tool_run().ran.lines[2], "Tool Used: editor");
}

#[test]
fn a_chat_logs_the_tool_result() {
    let run = tool_run();
    assert_eq!(
        run.ran.lines[4],
        format!("Tool Result: {}", created(&run.file))
    );
}

fn openai_call() -> Value {
    json!({"type": "function_call", "call_id": "c1", "name": "nope", "arguments": "{}"})
}

fn openai_message() -> Value {
    json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]})
}

fn openai_run() -> (Ran, Seen) {
    let (base, seen) = serve(vec![
        ok(json!({"output": [openai_call()]})),
        ok(json!({"output": [openai_message()]})),
    ]);
    (
        chat_on(
            client(Provider::OpenAi, "gpt-5", &base),
            |_| {},
            "do it",
            &[],
        ),
        seen,
    )
}

#[test]
fn openai_chats_count_every_call() {
    assert_eq!(openai_run().0.calls, 2);
}

#[test]
fn openai_chats_stop_without_a_call() {
    assert_eq!(
        openai_run().0.history,
        [
            json!({"role": "user", "content": [{"type": "input_text", "text": "do it"}]}),
            openai_call(),
            json!({"type": "function_call_output", "call_id": "c1", "output": "Error: Tool 'nope' not found"}),
            openai_message(),
        ]
    );
}

#[test]
fn openai_chats_use_the_responses_api() {
    assert_eq!(
        openai_run().1.lock().unwrap()[0].line,
        "POST /v1/responses HTTP/1.1\r\n"
    );
}

#[test]
fn openai_chats_name_the_model() {
    assert_eq!(body(&openai_run().1, 0)["model"], "gpt-5");
}

#[test]
fn openai_chats_start_with_a_system_input() {
    assert_eq!(body(&openai_run().1, 0)["input"][0]["role"], "system");
}

#[test]
fn openai_chats_use_strict_tools() {
    assert_eq!(body(&openai_run().1, 0)["tools"][0]["strict"], true);
}

fn limited() -> Ran {
    served(
        vec![ok(tool_call("c", "nope", &json!({})))],
        Provider::OpenRouter,
        "qwen",
        |chat| chat.max_llm_calls = 1,
    )
    .0
}

#[test]
fn a_chat_at_the_call_limit_keeps_the_exchange_so_far() {
    assert_eq!(limited().history.len(), 3);
}

#[test]
fn a_chat_stops_at_the_call_limit() {
    assert_eq!(limited().calls, 1);
}

fn on_refused(provider: Provider, model: &str, setup: impl FnOnce(&mut Chat)) -> Ran {
    chat_on(client(provider, model, &refused()), setup, "x", &[])
}

fn timed_out() -> Ran {
    on_refused(Provider::OpenRouter, "qwen", |chat| chat.timeout = 0.0)
}

#[test]
fn a_timed_out_chat_keeps_only_the_instruction() {
    assert_eq!(timed_out().history.len(), 1);
}

#[test]
fn a_timed_out_chat_makes_no_calls() {
    assert_eq!(timed_out().calls, 0);
}

#[test]
fn a_timed_out_chat_says_so() {
    assert_eq!(
        timed_out().lines[1],
        "Timeout reached, stopping further LLM calls."
    );
}

fn failing() -> Ran {
    on_refused(Provider::OpenRouter, "qwen", |chat| chat.retries = 1)
}

#[test]
fn a_failing_chat_keeps_only_the_instruction() {
    assert_eq!(failing().history.len(), 1);
}

#[test]
fn a_failing_chat_spends_its_call_budget() {
    assert_eq!(failing().calls, 1000);
}

#[test]
fn a_failing_chat_logs_each_retry() {
    assert_eq!(failing().lines.len(), 4);
}

#[test]
fn a_failing_chat_logs_the_response_error() {
    assert!(failing().lines[1].starts_with("Error in get_response_withtools: "));
}

#[test]
fn a_failing_chat_logs_the_chat_error() {
    assert!(failing().lines[3].starts_with("Error in chat_with_tools_open_router: "));
}

fn malformed(response: Value) -> Ran {
    served(vec![ok(response)], Provider::OpenRouter, "qwen", |_| {}).0
}

fn no_choices() -> Ran {
    malformed(json!({"choices": []}))
}

fn broken_call() -> Ran {
    malformed(json!({"choices": [{"message": {"tool_calls": [{"function": {"arguments": "{"}}]}}]}))
}

#[test]
fn a_reply_without_choices_ends_the_chat() {
    assert_eq!(no_choices().history.len(), 1);
}

#[test]
fn a_reply_without_choices_is_logged() {
    assert!(
        no_choices().lines[2]
            .starts_with("Error in chat_with_tools_open_router: unexpected response: no message")
    );
}

#[test]
fn a_broken_tool_call_ends_the_chat() {
    assert_eq!(broken_call().history.len(), 2);
}

#[test]
fn a_broken_tool_call_is_logged() {
    assert!(
        broken_call().lines[2]
            .starts_with("Error in chat_with_tools_open_router: unexpected response: ")
    );
}

fn empty_openai() -> Ran {
    served(vec![ok(json!({}))], Provider::OpenAi, "o3", |_| {}).0
}

#[test]
fn an_empty_openai_reply_ends_the_chat() {
    assert_eq!(
        empty_openai().history,
        [json!({"role": "user", "content": [{"type": "input_text", "text": "x"}]})]
    );
}

#[test]
fn an_empty_openai_reply_counts_as_a_call() {
    assert_eq!(empty_openai().calls, 1);
}

fn backed_off() -> (Ran, Seen) {
    let limited = (429, b"slow down".to_vec());
    served(
        vec![limited.clone(), limited, ok(reply("done"))],
        Provider::OpenRouter,
        "qwen",
        |chat| {
            chat.retries = 0;
            chat.backoff = Backoff {
                cap: Duration::from_millis(1),
                budget: Duration::from_secs(60),
            };
        },
    )
}

#[test]
fn rate_limits_are_not_counted_as_calls() {
    assert_eq!(backed_off().0.calls, 1);
}

#[test]
fn rate_limits_are_retried() {
    assert_eq!(backed_off().1.lock().unwrap().len(), 3);
}

#[test]
fn a_spent_backoff_budget_ends_the_chat() {
    let (ran, _) = served(
        vec![(429, b"slow down".to_vec())],
        Provider::OpenRouter,
        "qwen",
        |chat| {
            chat.retries = 0;
            chat.backoff.budget = Duration::ZERO;
        },
    );
    assert_eq!(ran.calls, 1000);
}

#[test]
fn backoff_waits_at_most_a_minute() {
    assert_eq!(Backoff::default().cap, Duration::from_secs(60));
}

fn unreachable(provider: Provider, model: &str) -> Ran {
    on_refused(provider, model, |chat| chat.retries = 0)
}

#[test]
fn an_unreachable_server_ends_a_chat() {
    assert_eq!(unreachable(Provider::Vllm, "vllm-x").calls, 1000);
}

#[test]
fn an_unreachable_server_is_logged_by_the_chat() {
    assert!(
        unreachable(Provider::Vllm, "vllm-x").lines[2]
            .starts_with("Error in chat_with_tools_open_router: ")
    );
}

#[test]
fn an_unreachable_server_ends_an_openai_chat() {
    assert_eq!(unreachable(Provider::OpenAi, "gpt-5").calls, 1000);
}

#[test]
fn an_unreachable_server_is_logged_by_the_openai_chat() {
    assert!(
        unreachable(Provider::OpenAi, "gpt-5").lines[2]
            .starts_with("Error in chat_with_tools_openai: ")
    );
}

#[test]
fn a_chat_caps_the_length_of_each_reply() {
    assert_eq!(first_request()["max_tokens"], MAX_OUTPUT_TOKENS);
}

fn cut_off() -> Value {
    json!({"choices": [{"finish_reason": "length", "message": {"role": "assistant", "content": "", "reasoning": "hmm"}}]})
}

fn openai_cut_off() -> Value {
    json!({"status": "incomplete", "incomplete_details": {"reason": "max_output_tokens"}, "output": [{"type": "reasoning"}]})
}

#[test]
fn a_length_finish_is_a_cut_off_reply() {
    assert!(is_cut_off(&cut_off(), "qwen"));
}

#[test]
fn a_stop_finish_is_not_a_cut_off_reply() {
    let stopped = json!({"choices": [{"finish_reason": "stop", "message": {"content": "done"}}]});
    assert!(!is_cut_off(&stopped, "qwen"));
}

#[test]
fn an_openai_reply_out_of_tokens_is_cut_off() {
    assert!(is_cut_off(&openai_cut_off(), "gpt-5"));
}

#[test]
fn an_openai_reply_incomplete_for_another_reason_is_not_cut_off() {
    let filtered =
        json!({"status": "incomplete", "incomplete_details": {"reason": "content_filter"}});
    assert!(!is_cut_off(&filtered, "gpt-5"));
}

fn resumed() -> (Ran, Seen) {
    served(
        vec![
            ok(cut_off()),
            ok(tool_call("c", "nope", &json!({}))),
            ok(reply("done")),
        ],
        Provider::OpenRouter,
        "qwen",
        |_| {},
    )
}

#[test]
fn a_cut_off_reply_does_not_end_the_chat() {
    assert_eq!(resumed().0.calls, 3);
}

#[test]
fn a_cut_off_reply_is_followed_by_a_request_for_a_tool_call() {
    assert_eq!(
        resumed().0.history[2],
        json!({"role": "user", "content": CUT_OFF})
    );
}

#[test]
fn a_cut_off_reply_stays_in_the_history() {
    assert_eq!(
        resumed().0.history[1],
        cut_off()["choices"][0]["message"].clone()
    );
}

#[test]
fn a_cut_off_reply_is_logged() {
    assert_eq!(
        resumed().0.lines[2],
        "Reply cut off at the length limit, asking for a tool call."
    );
}

#[test]
fn the_request_after_a_cut_off_carries_the_reminder() {
    assert_eq!(body(&resumed().1, 1)["messages"][3]["content"], CUT_OFF);
}

#[test]
fn a_chat_of_only_cut_off_replies_ends_at_the_call_limit() {
    let (ran, _) = served(
        vec![ok(cut_off()), ok(cut_off()), ok(cut_off())],
        Provider::OpenRouter,
        "qwen",
        |chat| chat.max_llm_calls = 3,
    );
    assert_eq!(ran.calls, 3);
}

fn openai_resumed() -> Ran {
    served(
        vec![
            ok(openai_cut_off()),
            ok(json!({"output": [openai_message()]})),
        ],
        Provider::OpenAi,
        "gpt-5",
        |_| {},
    )
    .0
}

#[test]
fn an_openai_cut_off_reply_does_not_end_the_chat() {
    assert_eq!(openai_resumed().calls, 2);
}

#[test]
fn an_openai_cut_off_reply_is_followed_by_a_request_for_a_tool_call() {
    assert_eq!(
        openai_resumed().history[2],
        json!({"role": "user", "content": [{"type": "input_text", "text": CUT_OFF}]})
    );
}

struct Stalled {
    ran: Ran,
    took: Duration,
}

fn stalled(provider: Provider, model: &str) -> Stalled {
    let start = std::time::Instant::now();
    let ran = chat_on(
        client(provider, model, &silent()),
        |chat| chat.timeout = 0.5,
        "x",
        &[],
    );
    Stalled {
        ran,
        took: start.elapsed(),
    }
}

#[test]
fn a_chat_with_an_unanswered_request_ends_within_its_timeout() {
    assert!(stalled(Provider::OpenRouter, "qwen").took < Duration::from_secs(2));
}

#[test]
fn a_timed_out_request_is_not_retried() {
    let lines = stalled(Provider::OpenRouter, "qwen").ran.lines;
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("Error in get_response_withtools: timed out"))
            .count(),
        1
    );
}

#[test]
fn a_timed_out_request_ends_the_chat() {
    assert!(
        stalled(Provider::OpenRouter, "qwen").ran.lines[2]
            .starts_with("Error in chat_with_tools_open_router: timed out")
    );
}

#[test]
fn an_unanswered_served_model_lookup_ends_within_the_timeout() {
    assert!(stalled(Provider::Vllm, "vllm-x").took < Duration::from_secs(2));
}

#[test]
fn an_unanswered_openai_request_ends_within_the_timeout() {
    assert!(stalled(Provider::OpenAi, "gpt-5").took < Duration::from_secs(2));
}

#[test]
fn a_spent_deadline_stops_the_retries() {
    let ran = on_refused(Provider::OpenRouter, "qwen", |chat| {
        chat.timeout = 0.000_001;
        chat.retries = 1000;
    });
    assert!(ran.lines.len() < 1000);
}

#[test]
fn a_passed_deadline_leaves_no_time_for_a_request() {
    assert_eq!(request_limit(std::time::Instant::now()), Duration::ZERO);
}

#[test]
fn a_request_waits_at_most_the_client_timeout() {
    let far = std::time::Instant::now() + Duration::from_secs(3600);
    assert_eq!(request_limit(far), Duration::from_secs(600));
}
