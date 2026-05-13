use modelrouter::{
    BillingMode, Capability, ProviderConfig, ProviderId, ProviderKind, build_http_provider_request,
    parse_openai_compatible_response, parse_openai_compatible_response_with_usage,
};

#[test]
fn builds_openai_compatible_chat_request() {
    let provider = ProviderConfig {
        id: ProviderId::LmStudio,
        model: "qwen3-coder".to_string(),
        enabled: true,
        kind: ProviderKind::OpenAiCompatible,
        billing: BillingMode::Local,
        endpoint_url: Some("http://localhost:1234/v1".to_string()),
        input_cost_per_million_tokens: 0.0,
        output_cost_per_million_tokens: 0.0,
        max_input_tokens: 128_000,
        capabilities: vec![Capability::Local, Capability::Code],
    };

    let request =
        build_http_provider_request(&provider, "Summarize this.").expect("request should build");

    assert_eq!(request.url, "http://localhost:1234/v1/chat/completions");
    assert_eq!(request.body["model"], "qwen3-coder");
    assert_eq!(request.body["stream"], false);
    assert_eq!(request.body["messages"][0]["role"], "user");
    assert_eq!(request.body["messages"][0]["content"], "Summarize this.");
}

#[test]
fn parses_openai_compatible_chat_response() {
    let body = r#"{
      "choices": [
        { "message": { "content": "Done." } }
      ]
    }"#;

    let output = parse_openai_compatible_response(body).expect("response should parse");

    assert_eq!(output, "Done.");
}

#[test]
fn parses_openai_compatible_response_usage_tokens() {
    let body = r#"{
      "choices": [
        { "message": { "content": "Done." } }
      ],
      "usage": {
        "prompt_tokens": 11,
        "completion_tokens": 7,
        "total_tokens": 18
      }
    }"#;

    let output = parse_openai_compatible_response_with_usage(body).expect("response should parse");

    assert_eq!(output.content, "Done.");
    assert_eq!(output.usage.expect("usage").input_tokens, 11);
}
