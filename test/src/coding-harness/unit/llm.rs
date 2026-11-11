use rustyhgm_coding_harness::llm::{
    Client, Error, MAX_OUTPUT_TOKENS, Provider, Service, create_client,
    extract_json_between_markers, get_response_from_llm, message, read_api_key,
};
use serde_json::{Value, json};

use crate::common::{Seen, cases, content, ok, refused, serve};

#[test]
fn extract_json_matches_the_python() {
    let mismatches: Vec<String> = cases("extract_json")
        .iter()
        .filter_map(|case| {
            let got = extract_json_between_markers(case["input"].as_str().unwrap())
                .unwrap_or(Value::Null);
            (got != case["output"]).then(|| format!("{}: {got}", case["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

fn key_file(text: &str) -> (tempfile::TempDir, Service) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("key");
    std::fs::write(&path, text).unwrap();
    let service = Service {
        provider: Provider::OpenRouter,
        base_url: "http://h/v1".into(),
        api_key_file: path,
    };
    (tmp, service)
}

#[test]
fn a_client_uses_the_service_it_is_given() {
    let (_tmp, service) = key_file("k");
    let c = create_client("m", &service).unwrap();
    assert_eq!(
        (c.provider, c.base_url, c.model),
        (
            Provider::OpenRouter,
            "http://h/v1".to_owned(),
            "m".to_owned()
        )
    );
}

#[test]
fn a_client_reads_its_key_from_the_file_without_the_newline() {
    let (_tmp, service) = key_file("secret\n");
    assert_eq!(
        create_client("m", &service).unwrap().api_key.as_deref(),
        Some("secret")
    );
}

#[test]
fn a_missing_key_file_is_an_error_naming_it() {
    let (tmp, mut service) = key_file("k");
    service.api_key_file = tmp.path().join("gone");
    assert!(
        create_client("m", &service)
            .unwrap_err()
            .starts_with(&format!("{}: ", tmp.path().join("gone").display()))
    );
}

#[test]
fn a_key_file_that_is_a_folder_is_an_error() {
    let (tmp, mut service) = key_file("k");
    service.api_key_file = tmp.path().to_path_buf();
    assert!(read_api_key(&service.api_key_file).is_err());
}

#[test]
fn each_provider_has_its_flag_name() {
    assert_eq!(
        [Provider::OpenAi, Provider::Vllm, Provider::OpenRouter].map(Provider::name),
        ["openai", "vllm", "openrouter"]
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

fn ask_o3() -> (String, Vec<Value>, Seen) {
    let (base, seen) = serve(vec![content("hi")]);
    let c = client(Provider::OpenAi, "o3", &format!("{base}/"), Some("k"));
    let history = [message("assistant", "before")];
    let (text, history) = get_response_from_llm("msg", &c, "sys ", &history, 0.5).unwrap();
    (text, history, seen)
}

#[test]
fn o_models_return_the_reply() {
    assert_eq!(ask_o3().0, "hi");
}

#[test]
fn o_models_fold_the_system_message_into_the_user_turn() {
    assert_eq!(
        ask_o3().1,
        [
            message("assistant", "before"),
            message("user", "sys msg"),
            message("assistant", "hi")
        ]
    );
}

#[test]
fn o_models_post_to_chat_completions() {
    let (_, _, seen) = ask_o3();
    assert_eq!(
        seen.lock().unwrap()[0].line,
        "POST /v1/chat/completions HTTP/1.1\r\n"
    );
}

#[test]
fn o_models_send_the_api_key() {
    let (_, _, seen) = ask_o3();
    assert_eq!(
        seen.lock().unwrap()[0].authorization.as_deref(),
        Some("Bearer k")
    );
}

#[test]
fn o_models_send_a_fixed_temperature_and_a_seed() {
    let (_, _, seen) = ask_o3();
    assert_eq!(
        body(&seen, 0),
        json!({"model": "o3", "messages": [message("assistant", "before"), message("user", "sys msg")], "temperature": 1, "n": 1, "seed": 0})
    );
}

fn ask_gpt() -> (String, Seen) {
    let (base, seen) = serve(vec![content("hey")]);
    let c = client(Provider::OpenAi, "GPT-5", &base, None);
    let (text, _) = get_response_from_llm("msg", &c, "sys", &[], 0.5).unwrap();
    (text, seen)
}

#[test]
fn gpt_models_return_the_reply() {
    assert_eq!(ask_gpt().0, "hey");
}

#[test]
fn a_client_without_a_key_sends_no_authorization() {
    assert_eq!(ask_gpt().1.lock().unwrap()[0].authorization, None);
}

#[test]
fn gpt_models_send_a_system_message_and_a_seed() {
    assert_eq!(
        body(&ask_gpt().1, 0),
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

fn ask_vllm() -> Seen {
    let (base, seen) = serve(vec![ok(json!({"data": [{"id": "served"}]})), content("ok")]);
    let c = client(Provider::Vllm, "vllm-host:x", &base, Some("dummy"));
    get_response_from_llm("msg", &c, "sys", &[], 0.0).unwrap();
    seen
}

#[test]
fn vllm_asks_the_server_which_model_it_serves() {
    assert_eq!(
        ask_vllm().lock().unwrap()[0].line,
        "GET /v1/models HTTP/1.1\r\n"
    );
}

#[test]
fn vllm_sends_the_key_when_listing_models() {
    assert_eq!(
        ask_vllm().lock().unwrap()[0].authorization.as_deref(),
        Some("Bearer dummy")
    );
}

#[test]
fn vllm_asks_for_the_served_model() {
    assert_eq!(body(&ask_vllm(), 1)["model"], "served");
}

#[test]
fn vllm_serving_no_model_is_a_response_error() {
    let (base, _) = serve(vec![ok(json!({"data": []}))]);
    let c = client(Provider::Vllm, "vllm-host:x", &base, None);
    let e = get_response_from_llm("msg", &c, "sys", &[], 0.0).unwrap_err();
    assert!(matches!(e, Error::Response(_)), "{e}");
}

#[test]
fn every_model_family_reports_a_failing_server() {
    let reports: Vec<String> = [
        (Provider::OpenAi, "o3"),
        (Provider::OpenAi, "gpt-5"),
        (Provider::OpenRouter, "m"),
        (Provider::Vllm, "vllm-h:m"),
    ]
    .into_iter()
    .map(|(provider, model)| {
        let (base, _) = serve(vec![(500, b"down".to_vec())]);
        let c = client(provider, model, &base, None);
        get_response_from_llm("msg", &c, "sys", &[], 0.0)
            .unwrap_err()
            .to_string()
    })
    .collect();
    assert_eq!(reports, vec!["Error code: 500 - down"; 4]);
}

#[test]
fn vllm_reports_a_server_failing_after_listing_models() {
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

fn reply(response: (u16, Vec<u8>)) -> Result<String, Error> {
    let (base, _) = serve(vec![response]);
    client(Provider::OpenRouter, "m", &base, None).chat(&json!({}))
}

#[test]
fn a_reply_that_is_not_json_is_quoted_in_the_error() {
    let e = reply((200, b"not json".to_vec())).unwrap_err().to_string();
    assert!(
        e.strip_prefix("unexpected response: ")
            .is_some_and(|rest| rest.ends_with(": not json")),
        "{e}"
    );
}

#[test]
fn a_reply_that_is_not_text_is_an_http_error() {
    assert!(matches!(reply((200, vec![0xff])), Err(Error::Http(_))));
}

#[test]
fn a_reply_without_choices_is_a_response_error() {
    assert!(matches!(
        reply(ok(json!({"choices": []}))),
        Err(Error::Response(_))
    ));
}

fn dead() -> Client {
    client(Provider::OpenRouter, "m", &refused(), None)
}

#[test]
fn posting_to_a_dead_server_is_an_http_error() {
    assert!(matches!(dead().post("p", &json!({})), Err(Error::Http(_))));
}

#[test]
fn getting_from_a_dead_server_is_an_http_error() {
    assert!(matches!(dead().get("p"), Err(Error::Http(_))));
}

#[test]
fn an_http_error_says_what_went_wrong() {
    assert!(!dead().get("p").unwrap_err().to_string().is_empty());
}
