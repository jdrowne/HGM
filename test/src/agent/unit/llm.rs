use hgm_agent::llm::{
    Client, Error, MAX_OUTPUT_TOKENS, Provider, create_client, extract_json_between_markers,
    get_response_from_llm, message,
};
use serde_json::{Value, json};

use crate::common::cases;
use crate::stub::{Seen, content, ok, refused, serve};

#[test]
fn extract_json_matches_the_python() {
    for case in cases("extract_json") {
        let got = extract_json_between_markers(case["input"].as_str().unwrap());
        assert_eq!(
            got.unwrap_or(Value::Null),
            case["output"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn clients_follow_the_model_name() {
    assert_eq!(create_client("gpt-5").provider, Provider::OpenAi);
    assert_eq!(create_client("o3").provider, Provider::OpenAi);
    let vllm = create_client("vllm-host1:qwen");
    assert_eq!(vllm.provider, Provider::Vllm);
    assert!(vllm.base_url.ends_with(":8000/v1"));
    assert_eq!(
        create_client("anthropic/claude-sonnet-4").provider,
        Provider::OpenRouter
    );
}

fn client(provider: Provider, model: &str, base_url: &str, api_key: Option<&str>) -> Client {
    Client {
        provider,
        model: model.to_owned(),
        base_url: base_url.to_owned(),
        api_key: api_key.map(str::to_owned),
    }
}

fn body(seen: &Seen, i: usize) -> Value {
    serde_json::from_str(&seen.lock().unwrap()[i].body).unwrap()
}

#[test]
fn o_models_fold_the_system_message_into_the_user_turn() {
    let (base, seen) = serve(vec![content("hi")]);
    let c = client(Provider::OpenAi, "o3", &format!("{base}/"), Some("k"));
    let history = [message("assistant", "before")];
    let (text, history) = get_response_from_llm("msg", &c, "sys ", &history, 0.5).unwrap();
    assert_eq!(text, "hi");
    assert_eq!(
        history,
        [
            message("assistant", "before"),
            message("user", "sys msg"),
            message("assistant", "hi")
        ]
    );
    let request = seen.lock().unwrap()[0].clone();
    assert_eq!(request.line, "POST /v1/chat/completions HTTP/1.1\r\n");
    assert_eq!(request.authorization.as_deref(), Some("Bearer k"));
    assert_eq!(
        body(&seen, 0),
        json!({"model": "o3", "messages": [message("assistant", "before"), message("user", "sys msg")], "temperature": 1, "n": 1, "seed": 0})
    );
}

#[test]
fn gpt_models_send_a_system_message_and_a_seed() {
    let (base, seen) = serve(vec![content("hey")]);
    let c = client(Provider::OpenAi, "GPT-5", &base, None);
    let (text, _) = get_response_from_llm("msg", &c, "sys", &[], 0.5).unwrap();
    assert_eq!(text, "hey");
    assert_eq!(seen.lock().unwrap()[0].authorization, None);
    assert_eq!(
        body(&seen, 0),
        json!({"model": "GPT-5", "messages": [message("system", "sys"), message("user", "msg")], "n": 1, "stop": null, "seed": 0})
    );
}

#[test]
fn other_models_send_temperature_and_max_tokens() {
    let (base, seen) = serve(vec![content("yo")]);
    let c = client(Provider::OpenRouter, "anthropic/claude", &base, Some("k"));
    get_response_from_llm("msg", &c, "sys", &[], 0.25).unwrap();
    assert_eq!(
        body(&seen, 0),
        json!({"model": "anthropic/claude", "messages": [message("system", "sys"), message("user", "msg")], "temperature": 0.25, "max_tokens": MAX_OUTPUT_TOKENS, "n": 1, "stop": null})
    );
}

#[test]
fn vllm_asks_the_server_which_model_it_serves() {
    let (base, seen) = serve(vec![ok(json!({"data": [{"id": "served"}]})), content("ok")]);
    let c = client(Provider::Vllm, "vllm-host:x", &base, Some("dummy"));
    get_response_from_llm("msg", &c, "sys", &[], 0.0).unwrap();
    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests[0].line, "GET /v1/models HTTP/1.1\r\n");
    assert_eq!(requests[0].authorization.as_deref(), Some("Bearer dummy"));
    assert_eq!(body(&seen, 1)["model"], "served");
    let (base, _) = serve(vec![ok(json!({"data": []}))]);
    let c = client(Provider::Vllm, "vllm-host:x", &base, None);
    let e = get_response_from_llm("msg", &c, "sys", &[], 0.0).unwrap_err();
    assert!(matches!(e, Error::Response(_)), "{e}");
}

#[test]
fn every_model_family_reports_a_failing_server() {
    for (provider, model) in [
        (Provider::OpenAi, "o3"),
        (Provider::OpenAi, "gpt-5"),
        (Provider::OpenRouter, "m"),
        (Provider::Vllm, "vllm-h:m"),
    ] {
        let (base, _) = serve(vec![(500, b"down".to_vec())]);
        let c = client(provider, model, &base, None);
        let e = get_response_from_llm("msg", &c, "sys", &[], 0.0).unwrap_err();
        assert_eq!(e.to_string(), "Error code: 500 - down", "{model}");
    }
    let (base, _) = serve(vec![
        ok(json!({"data": [{"id": "m"}]})),
        (503, b"busy".to_vec()),
    ]);
    let c = client(Provider::Vllm, "vllm-h:m", &base, None);
    assert!(matches!(
        get_response_from_llm("m", &c, "s", &[], 0.0),
        Err(Error::Status { code: 503, .. })
    ));
}

#[test]
fn malformed_replies_and_dead_servers_are_errors() {
    let (base, _) = serve(vec![
        (200, b"not json".to_vec()),
        (200, vec![0xff]),
        ok(json!({"choices": []})),
    ]);
    let c = client(Provider::OpenRouter, "m", &base, None);
    let e = c.chat(&json!({})).unwrap_err();
    assert!(
        e.to_string().starts_with("unexpected response: ") && e.to_string().ends_with(": not json"),
        "{e}"
    );
    assert!(matches!(c.chat(&json!({})), Err(Error::Http(_))));
    assert!(matches!(c.chat(&json!({})), Err(Error::Response(_))));
    let dead = client(Provider::OpenRouter, "m", &refused(), None);
    assert!(matches!(dead.post("p", &json!({})), Err(Error::Http(_))));
    let e = dead.get("p").unwrap_err();
    assert!(matches!(e, Error::Http(_)));
    assert!(!e.to_string().is_empty());
}
