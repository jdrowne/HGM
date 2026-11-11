use std::cell::RefCell;
use std::fs;
use std::time::Duration;

use hgm_agent::llm::{Client, Provider};
use hgm_agent::tools::tool_infos;
use hgm_agent::withtools::{
    Backoff, Chat, SYSTEM_MESSAGE, ToolUse, check_for_tool_use, convert_msg_history,
    convert_tool_info, is_openai,
};
use serde_json::{Value, json};

use crate::common::{Seen, cases, ok, refused, serve};

#[test]
fn tool_conversion_matches_the_python() {
    for c in cases("tool_conversion") {
        let input = &c["input"];
        let got = convert_tool_info(&input["tool_info"], input["model"].as_str().unwrap());
        assert_eq!(got, c["output"], "{}", c["name"]);
    }
    let odd = json!({"name": "n", "description": "d", "input_schema": {"properties": {"a": {"type": 1}, "b": {}}}});
    assert_eq!(
        convert_tool_info(&odd, "gpt-5")["parameters"],
        json!({"properties": {"a": {"type": 1}, "b": {"type": null}}, "additionalProperties": false, "required": ["a", "b"]})
    );
    let bare = json!({"name": "n", "description": "d", "input_schema": {}});
    assert_eq!(convert_tool_info(&bare, "gpt-5")["parameters"], json!({}));
    assert_eq!(
        convert_tool_info(&bare, "qwen")["function"]["parameters"]["required"],
        json!([])
    );
    assert!(is_openai("o3") && is_openai("my-GPT") && !is_openai("qwen"));
}

#[test]
fn message_histories_convert_like_the_python() {
    for c in cases("msg_history") {
        let input = &c["input"];
        let history = input["msg_history"].as_array().unwrap();
        let got = convert_msg_history(history, input["model"].as_str().unwrap());
        assert_eq!(Value::Array(got), c["output"], "{}", c["name"]);
    }
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

#[test]
fn tool_calls_are_found_in_either_response_shape() {
    let chat = json!({"choices": [{"message": {"tool_calls": [{"id": "i", "function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}]}}]});
    let want = ToolUse {
        id: "i".to_owned(),
        name: "bash".to_owned(),
        input: json!({"command": "ls"}),
    };
    assert_eq!(
        check_for_tool_use(&chat, "qwen").unwrap(),
        Some(want.clone())
    );
    let empty = json!({"choices": [{"message": {"tool_calls": []}}]});
    assert_eq!(check_for_tool_use(&empty, "qwen").unwrap(), None);
    let openai = json!({"output": [{"type": "reasoning"}, {"type": "function_call", "call_id": "i", "name": "bash", "arguments": "{\"command\": \"ls\"}"}]});
    assert_eq!(check_for_tool_use(&openai, "gpt-5").unwrap(), Some(want));
    let text = json!({"output": [{"type": "message", "role": "assistant", "content": []}]});
    assert_eq!(check_for_tool_use(&text, "gpt-5").unwrap(), None);
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

#[test]
fn a_chat_runs_tools_until_the_model_stops() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("f.txt");
    let edit = json!({"command": "create", "path": file, "file_text": "hi"});
    let (base, seen) = serve(vec![
        ok(json!({"data": [{"id": "served"}]})),
        ok(tool_call("c1", "editor", &edit)),
        ok(json!({"data": [{"id": "served"}]})),
        ok(reply("done")),
    ]);
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let chat = Chat::new(client(Provider::Vllm, "vllm-x", &base), &log);
    let before = [json!({"role": "user", "content": "earlier"})];
    let (history, calls) = chat.run("do it", &before);
    assert_eq!(calls, 2);
    assert_eq!(fs::read_to_string(&file).unwrap(), "hi");
    let result = format!("File created successfully at: {}", file.display());
    assert_eq!(
        history,
        [
            before[0].clone(),
            json!({"role": "user", "content": "do it"}),
            tool_call("c1", "editor", &edit)["choices"][0]["message"].clone(),
            json!({"role": "tool", "tool_call_id": "c1", "name": "editor", "content": result}),
            reply("done")["choices"][0]["message"].clone(),
        ]
    );
    let first = body(&seen, 1);
    assert_eq!(first["model"], "served");
    assert_eq!(first["tool_choice"], "auto");
    assert_eq!(first["parallel_tool_calls"], false);
    assert_eq!(
        first["messages"][0],
        json!({"role": "system", "content": SYSTEM_MESSAGE})
    );
    assert_eq!(first["messages"].as_array().unwrap().len(), 3);
    let tools: Vec<Value> = tool_infos()
        .iter()
        .map(|t| convert_tool_info(t, "vllm-x"))
        .collect();
    assert_eq!(first["tools"], json!(tools));
    assert_eq!(body(&seen, 3)["messages"].as_array().unwrap().len(), 5);
    let lines = lines.borrow();
    assert_eq!(lines[0], "\n========== User Instruction ==========\ndo it");
    assert!(lines[1].starts_with("Tool Response: {"));
    assert_eq!(lines[2], "Tool Used: editor");
    assert_eq!(lines[4], format!("Tool Result: {result}"));
}

#[test]
fn openai_models_use_the_responses_api_and_stop_without_a_call() {
    let call = json!({"type": "function_call", "call_id": "c1", "name": "nope", "arguments": "{}"});
    let message = json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]});
    let (base, seen) = serve(vec![
        ok(json!({"output": [call]})),
        ok(json!({"output": [message]})),
    ]);
    let log = |_: &str| {};
    let chat = Chat::new(client(Provider::OpenAi, "gpt-5", &base), &log);
    let (history, calls) = chat.run("do it", &[]);
    assert_eq!(calls, 2);
    assert_eq!(
        history,
        [
            json!({"role": "user", "content": [{"type": "input_text", "text": "do it"}]}),
            call,
            json!({"type": "function_call_output", "call_id": "c1", "output": "Error: Tool 'nope' not found"}),
            message,
        ]
    );
    assert_eq!(
        seen.lock().unwrap()[0].line,
        "POST /v1/responses HTTP/1.1\r\n"
    );
    let first = body(&seen, 0);
    assert_eq!(first["model"], "gpt-5");
    assert_eq!(first["input"][0]["role"], "system");
    assert_eq!(first["tools"][0]["strict"], true);
}

#[test]
fn chats_stop_at_the_call_limit_the_timeout_or_an_error() {
    let (base, _) = serve(vec![ok(tool_call("c", "nope", &json!({})))]);
    let log = |_: &str| {};
    let mut chat = Chat::new(client(Provider::OpenRouter, "qwen", &base), &log);
    chat.max_llm_calls = 1;
    let (history, calls) = chat.run("x", &[]);
    assert_eq!((history.len(), calls), (3, 1));

    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let mut chat = Chat::new(client(Provider::OpenRouter, "qwen", &refused()), &log);
    chat.timeout = 0.0;
    let (history, calls) = chat.run("x", &[]);
    assert_eq!((history.len(), calls), (1, 0));
    assert_eq!(
        lines.borrow()[1],
        "Timeout reached, stopping further LLM calls."
    );

    lines.borrow_mut().clear();
    chat.timeout = 3600.0;
    chat.retries = 1;
    let (history, calls) = chat.run("x", &[]);
    assert_eq!((history.len(), calls), (1, 1000));
    let lines = lines.borrow();
    assert_eq!(lines.len(), 4);
    assert!(lines[1].starts_with("Error in get_response_withtools: "));
    assert!(lines[3].starts_with("Error in chat_with_agent_open_router: "));
}

#[test]
fn malformed_responses_end_the_chat() {
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    let (base, _) = serve(vec![
        ok(json!({"choices": []})),
        ok(json!({"choices": [{"message": {"tool_calls": [{"function": {"arguments": "{"}}]}}]})),
    ]);
    let chat = Chat::new(client(Provider::OpenRouter, "qwen", &base), &log);
    assert_eq!(chat.run("x", &[]).0.len(), 1);
    assert_eq!(chat.run("x", &[]).0.len(), 2);
    let (base, _) = serve(vec![ok(json!({}))]);
    let openai = Chat::new(client(Provider::OpenAi, "o3", &base), &log);
    assert_eq!(
        openai.run("x", &[]),
        (
            vec![json!({"role": "user", "content": [{"type": "input_text", "text": "x"}]})],
            1
        )
    );
    let lines = lines.borrow();
    assert!(
        lines[2]
            .starts_with("Error in chat_with_agent_open_router: unexpected response: no message")
    );
    assert!(lines[5].starts_with("Error in chat_with_agent_open_router: unexpected response: "));
}

#[test]
fn rate_limits_back_off_until_the_budget_is_spent() {
    let limited = (429, b"slow down".to_vec());
    let (base, seen) = serve(vec![limited.clone(), limited.clone(), ok(reply("done"))]);
    let log = |_: &str| {};
    let mut chat = Chat::new(client(Provider::OpenRouter, "qwen", &base), &log);
    chat.retries = 0;
    chat.backoff = Backoff {
        cap: Duration::from_millis(1),
        budget: Duration::from_secs(60),
    };
    assert_eq!(chat.run("x", &[]).1, 1);
    assert_eq!(seen.lock().unwrap().len(), 3);
    let (base, _) = serve(vec![limited]);
    let mut chat = Chat::new(client(Provider::OpenRouter, "qwen", &base), &log);
    chat.retries = 0;
    chat.backoff.budget = Duration::ZERO;
    assert_eq!(chat.run("x", &[]).1, 1000);
    assert_eq!(Backoff::default().cap, Duration::from_secs(60));
}

#[test]
fn unreachable_servers_end_either_kind_of_chat() {
    let lines = RefCell::new(Vec::new());
    let log = |m: &str| lines.borrow_mut().push(m.to_owned());
    for (provider, model) in [(Provider::Vllm, "vllm-x"), (Provider::OpenAi, "gpt-5")] {
        let mut chat = Chat::new(client(provider, model, &refused()), &log);
        chat.retries = 0;
        assert_eq!(chat.run("x", &[]).1, 1000);
    }
    let lines = lines.borrow();
    assert!(lines[2].starts_with("Error in chat_with_agent_open_router: "));
    assert!(lines[5].starts_with("Error in chat_with_agent_openai: "));
}
