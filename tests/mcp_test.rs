use std::path::Path;

use modelrouter::{ProviderConfig, RouterConfig, handle_mcp_message_with_runner};

#[test]
fn mcp_initialize_returns_server_info() {
    let config = RouterConfig::default();
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
    )
    .expect("initialize response");

    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["serverInfo"]["name"], "modelrouter");
}

#[test]
fn mcp_lists_route_and_run_tools() {
    let config = RouterConfig::default();
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
    )
    .expect("tools/list response");

    let tools = response["result"]["tools"].as_array().expect("tools array");
    assert!(tools.iter().any(|tool| tool["name"] == "modelrouter_route"));
    assert!(tools.iter().any(|tool| tool["name"] == "modelrouter_run"));
    assert!(
        tools
            .iter()
            .any(|tool| tool["name"] == "modelrouter_broker")
    );
}

#[test]
fn mcp_route_tool_returns_decision_content() {
    let config = RouterConfig::default();
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"modelrouter_route","arguments":{"prompt":"Fix parser tests in this repo"}}}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
    )
    .expect("tools/call response");

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("content text");
    assert!(text.contains("\"provider\":\"codex\""));
}

#[test]
fn mcp_route_tool_uses_llm_classifier_when_enabled() {
    let mut config = RouterConfig::default();
    config.classification.mode = modelrouter::ClassificationMode::Llm;
    config.classification.provider = Some(modelrouter::ProviderId::Local);
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"modelrouter_route","arguments":{"prompt":"Please handle this ambiguous request."}}}"#,
        |provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            assert_eq!(provider.id, modelrouter::ProviderId::Local);
            assert!(prompt.contains("Classify this request"));
            Ok(r#"{"task":"code","repo":true,"confidence":0.9}"#.to_string())
        },
    )
    .expect("tools/call response");

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("content text");
    assert!(text.contains("\"provider\":\"codex\""));
    assert!(text.contains("LLM classifier supplied routing signals"));
}

#[test]
fn mcp_run_tool_uses_runner() {
    let config = RouterConfig::default();
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"modelrouter_run","arguments":{"prompt":"Summarize this private note"}}}"#,
        |_provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("ran: {prompt}"))
        },
    )
    .expect("tools/call response");

    assert_eq!(
        response["result"]["content"][0]["text"],
        "ran: Summarize this private note"
    );
}

#[test]
fn mcp_broker_tool_returns_decision_and_output() {
    let config = RouterConfig::default();
    let response = handle_mcp_message_with_runner(
        &config,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"modelrouter_broker","arguments":{"prompt":"Fix the failing tests in this repo"}}}"#,
        |_provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("brokered: {prompt}"))
        },
    )
    .expect("tools/call response");

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("content text");
    assert!(text.contains(r#""provider":"codex""#));
    assert!(text.contains(r#""output":"brokered: Fix the failing tests in this repo""#));
}
