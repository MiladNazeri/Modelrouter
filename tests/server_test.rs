use std::path::Path;

use modelrouter::{
    ProviderConfig, RouterConfig, handle_server_request, handle_server_request_with_config_path,
    handle_server_request_with_directory_picker,
    handle_server_request_with_headers_runner_and_report,
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
fn oversized_post_body_is_rejected_before_routing() {
    let config = RouterConfig::default();
    let prompt = "x".repeat(modelrouter::MAX_REQUEST_BODY_BYTES + 1);
    let body = format!(r#"{{"prompt":"{prompt}"}}"#);

    let response = handle_server_request(&config, "POST", "/route", &body);

    assert_eq!(response.status, 413);
    assert_eq!(response.body["error"], "request_too_large");
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
    assert!(
        !response
            .body
            .to_string()
            .contains("http://localhost:1234/v1")
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
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("modelrouter doctor")
    );
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("Choose folder")
    );
    assert!(
        response.body["html"]
            .as_str()
            .expect("html")
            .contains("/fs/pick-directory")
    );
    assert!(
        !response.body["html"]
            .as_str()
            .expect("html")
            .contains("innerHTML")
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
    assert!(response.body.get("prompt").is_none());
    assert!(response.body.get("output").is_none());
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

#[test]
fn configured_auth_token_blocks_run_without_bearer_token() {
    let mut config = RouterConfig::default();
    config.server.auth_token = Some("secret-token".to_string());

    let response = modelrouter::handle_server_request_with_headers(
        &config,
        "POST",
        "/run",
        r#"{"prompt":"Summarize this note."}"#,
        &[],
    );

    assert_eq!(response.status, 401);
    assert_eq!(response.body["error"], "unauthorized");
}

#[test]
fn configured_auth_token_accepts_matching_bearer_token() {
    let mut config = RouterConfig::default();
    config.server.auth_token = Some("secret-token".to_string());
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_headers_runner_and_report(
        &config,
        "POST",
        "/run",
        r#"{"prompt":"Summarize this note."}"#,
        &[("authorization", "Bearer secret-token")],
        |_provider: &ProviderConfig, prompt: &str, _cwd: Option<&Path>| {
            Ok(format!("authorized: {prompt}"))
        },
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["output"], "authorized: Summarize this note.");
}

#[test]
fn fs_endpoint_lists_directory_entries_for_picker() {
    let config = RouterConfig::default();
    let dir = tempfile::tempdir().expect("temp dir");
    let child = dir.path().join("child-project");
    std::fs::create_dir(&child).expect("child dir");
    std::fs::write(dir.path().join("README.md"), "hello").expect("file");
    let path = dir.path().to_str().expect("utf8 path").replace('/', "%2F");
    let canonical = std::fs::canonicalize(dir.path()).expect("canonical temp dir");

    let response = handle_server_request(&config, "GET", &format!("/fs?path={path}"), "");

    assert_eq!(response.status, 200);
    assert_eq!(response.body["path"], canonical.display().to_string());
    assert_eq!(response.body["entries"][0]["name"], "child-project");
    assert_eq!(response.body["entries"][0]["kind"], "directory");
    assert_eq!(response.body["entries"][1]["name"], "README.md");
    assert_eq!(response.body["entries"][1]["kind"], "file");
}

#[test]
fn fs_pick_directory_endpoint_returns_native_selection() {
    let config = RouterConfig::default();
    let dir = tempfile::tempdir().expect("temp dir");
    let canonical = std::fs::canonicalize(dir.path()).expect("canonical temp dir");

    let response = handle_server_request_with_directory_picker(
        &config,
        "POST",
        "/fs/pick-directory",
        "{}",
        || Ok(Some(dir.path().to_path_buf())),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["cancelled"], false);
    assert_eq!(response.body["path"], canonical.display().to_string());
}

#[test]
fn fs_pick_directory_endpoint_reports_cancelled_selection() {
    let config = RouterConfig::default();

    let response = handle_server_request_with_directory_picker(
        &config,
        "POST",
        "/fs/pick-directory",
        "{}",
        || Ok(None),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["cancelled"], true);
    assert!(response.body["path"].is_null());
}

#[test]
fn fs_endpoint_requires_auth_when_server_token_is_configured() {
    let mut config = RouterConfig::default();
    config.server.auth_token = Some("secret-token".to_string());

    let response = handle_server_request(&config, "GET", "/fs", "");

    assert_eq!(response.status, 401);
    assert_eq!(response.body["error"], "unauthorized");
}

#[test]
fn fs_pick_directory_endpoint_requires_auth_when_server_token_is_configured() {
    let mut config = RouterConfig::default();
    config.server.auth_token = Some("secret-token".to_string());

    let response = handle_server_request(&config, "POST", "/fs/pick-directory", "{}");

    assert_eq!(response.status, 401);
    assert_eq!(response.body["error"], "unauthorized");
}

#[test]
fn config_endpoint_returns_editable_toml_and_profiles() {
    let config = RouterConfig::default();

    let response = handle_server_request(&config, "GET", "/config", "");

    assert_eq!(response.status, 200);
    assert!(
        response.body["toml"]
            .as_str()
            .expect("toml")
            .contains("[routing]")
    );
    assert!(response.body["config"]["providers"].is_array());
    assert!(response.body["config"]["profiles"].is_array());
}

#[test]
fn config_validate_rejects_invalid_toml_without_saving() {
    let config = RouterConfig::default();

    let response = handle_server_request(
        &config,
        "POST",
        "/config/validate",
        r#"{"toml":"not = [valid"}"#,
    );

    assert_eq!(response.status, 422);
    assert_eq!(response.body["valid"], false);
}

#[test]
fn config_provider_update_saves_and_reloads_provider_settings() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    let mut config = RouterConfig::default();
    std::fs::write(&config_path, toml::to_string_pretty(&config).expect("toml"))
        .expect("write config");

    let response = handle_server_request_with_config_path(
        &mut config,
        "POST",
        "/config/provider",
        r#"{"provider":"gemini","enabled":false,"model":"gemini-test"}"#,
        Some(&config_path),
    );

    assert_eq!(response.status, 200);
    let gemini = config
        .provider(modelrouter::ProviderId::Gemini)
        .expect("gemini");
    assert!(!gemini.enabled);
    assert_eq!(gemini.model, "gemini-test");
    let saved = std::fs::read_to_string(&config_path).expect("saved config");
    assert!(saved.contains("model = \"gemini-test\""));
    assert!(dir.path().join("modelrouter.toml.bak").is_file());
}

#[test]
fn profile_update_adds_project_profile_and_persists_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    let mut config = RouterConfig::default();
    std::fs::write(&config_path, toml::to_string_pretty(&config).expect("toml"))
        .expect("write config");

    let response = handle_server_request_with_config_path(
        &mut config,
        "POST",
        "/config/profile",
        r#"{"name":"new-app","path_contains":"NewApp","default_provider":"claude","code_provider":"codex","reasoning_provider":"claude","local_provider":"local"}"#,
        Some(&config_path),
    );

    assert_eq!(response.status, 200);
    assert!(
        config
            .profiles
            .iter()
            .any(|profile| profile.name == "new-app")
    );
    let saved = std::fs::read_to_string(&config_path).expect("saved config");
    assert!(saved.contains("name = \"new-app\""));
}

#[test]
fn provider_test_endpoint_returns_single_provider_health() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |_| true, |_| true);

    let response = handle_server_request_with_runner_and_report(
        &config,
        "POST",
        "/provider-test",
        r#"{"provider":"codex"}"#,
        |_provider: &ProviderConfig, _prompt: &str, _cwd: Option<&Path>| Ok(String::new()),
        Some(&report),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.body["provider"], "codex");
    assert_eq!(response.body["status"], "available");
}
