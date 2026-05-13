use std::path::Path;

use modelrouter::{
    ProviderConfig, RouterConfig, handle_server_request,
    handle_server_request_with_runner_and_report, health_report_with,
};

#[test]
fn route_endpoint_returns_decision_json() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/route",
        r#"{"prompt":"Fix the failing parser tests in this repo"}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["provider"], "codex");
    assert_eq!(response.body["billing"], "subscription");
}

#[test]
fn run_endpoint_uses_selected_provider_runner() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/run",
        r#"{"prompt":"Summarize this private note."}"#,
        |_provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("handled: {prompt}"))
        },
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["decision"]["provider"], "local");
    assert_eq!(
        response.body["output"],
        "handled: Summarize this private note."
    );
}

#[test]
fn run_endpoint_can_prefer_gemini() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/run",
        r#"{"prompt":"Analyze this dossier.","prefer":"gemini"}"#,
        |provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("provider={}", provider.id))
        },
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["decision"]["provider"], "gemini");
    assert_eq!(response.body["output"], "provider=gemini");
}

#[test]
fn unknown_endpoint_returns_not_found() {
    let config = RouterConfig::default();

    let response = handle_server_request(&config, "POST", "/missing", "{}");

    assert_eq!(response.status, 404);
    assert_eq!(response.body["error"], "not_found");
}

#[test]
fn health_endpoint_returns_provider_status() {
    let config = RouterConfig::default();

    let response = handle_server_request(&config, "GET", "/health", "");

    assert_eq!(response.status, 200);
    assert!(
        response.body["providers"]
            .as_array()
            .expect("providers")
            .len()
            >= 3
    );
}

#[test]
fn gui_endpoint_returns_html() {
    let config = RouterConfig::default();

    let response = handle_server_request(&config, "GET", "/", "");

    assert_eq!(response.status, 200);
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("Model Router")
    );
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("Queue")
    );
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("Metrics")
    );
}

#[test]
fn queue_endpoint_returns_completed_job() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/queue",
        r#"{"prompt":"Summarize this quick note."}"#,
        |_provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("queued: {prompt}"))
        },
        Some(&report),
    );

    assert_eq!(response.status, 202);
    assert_eq!(response.body["status"], "completed");
    assert_eq!(
        response.body["output"],
        "queued: Summarize this quick note."
    );
}

#[test]
fn metrics_endpoint_returns_empty_metrics_without_log_path() {
    let config = RouterConfig::default();

    let response = handle_server_request(&config, "GET", "/metrics", "");

    assert_eq!(response.status, 200);
    assert_eq!(response.body["total_requests"], 0);
}

#[test]
fn spend_and_budget_endpoints_return_tracking_state() {
    let mut config = RouterConfig::default();
    config.budget.monthly_api_budget_cents = Some(1_000.0);

    let spend = handle_server_request(&config, "GET", "/spend", "");
    let budget = handle_server_request(&config, "GET", "/budget", "");

    assert_eq!(spend.status, 200);
    assert_eq!(spend.body["total_requests"], 0);
    assert_eq!(budget.status, 200);
    assert_eq!(budget.body["monthly_api_budget_cents"], 1_000.0);
}
