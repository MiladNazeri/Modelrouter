use std::path::Path;

use modelrouter::{
    ProviderConfig, RouterConfig, handle_server_request_with_runner_and_report, health_report_with,
};

#[test]
fn openai_compatible_proxy_routes_and_returns_chat_completion() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/v1/chat/completions",
        r#"{
          "model": "modelrouter",
          "messages": [
            {"role": "system", "content": "Be terse."},
            {"role": "user", "content": "Fix the failing tests in this repo"}
          ],
          "stream": false
        }"#,
        |provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!(
                "{} handled {}",
                provider.id,
                prompt.contains("failing tests")
            ))
        },
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["choices"][0]["message"]["content"],
        "codex handled true"
    );
    assert_eq!(response.body["modelrouter"]["provider"], "codex");
}

#[test]
fn openai_proxy_rejects_streaming_for_subscription_cli_safety() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/v1/chat/completions",
        r#"{"model":"modelrouter","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
        Some(&report),
    );

    assert_eq!(response.status, 400);
    assert_eq!(response.body["error"]["code"], "streaming_not_supported");
}
