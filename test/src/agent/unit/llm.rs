use hgm_agent::llm::{Provider, create_client, extract_json_between_markers};
use serde_json::Value;

use crate::common::cases;

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
