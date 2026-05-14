use std::{
    env, fs,
    io::Read,
    net::ToSocketAddrs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::{
    ExecutionQueue, FeedbackEntry, GUI_HTML, PathFavorite, ProjectProfile, ProviderConfig,
    ProviderHealth, ProviderId, RequestLogEntry, RouteDecision, RouteMatch, RouteRequest,
    RouteRule, Router, RouterConfig, TaskHint, append_feedback, append_request_log,
    apply_project_profile, availability_filtered_config, health_report, load_metrics,
    metrics_from_logs, run_provider,
};

pub const MAX_REQUEST_BODY_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ServerResponse {
    pub status: u16,
    pub body: Value,
    pub content_type: &'static str,
}

#[derive(Clone, Debug, Deserialize)]
struct RoutePayload {
    prompt: String,
    #[serde(default)]
    prefer: Option<ProviderId>,
    #[serde(default)]
    max_cost_cents: Option<f64>,
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    hint: Option<TaskHint>,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
struct RunResponse {
    decision: RouteDecision,
    output: String,
}

#[derive(Clone, Debug, Deserialize)]
struct OpenAiChatPayload {
    #[serde(default)]
    model: Option<String>,
    messages: Vec<OpenAiMessage>,
    #[serde(default)]
    stream: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ConfigTomlPayload {
    toml: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ProviderTestPayload {
    provider: ProviderId,
}

#[derive(Clone, Debug, Deserialize)]
struct ProviderUpdatePayload {
    provider: ProviderId,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    endpoint_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ProfileUpdatePayload {
    name: String,
    path_contains: String,
    #[serde(default)]
    default_provider: Option<ProviderId>,
    #[serde(default)]
    code_provider: Option<ProviderId>,
    #[serde(default)]
    reasoning_provider: Option<ProviderId>,
    #[serde(default)]
    local_provider: Option<ProviderId>,
}

#[derive(Clone, Debug, Deserialize)]
struct SettingsUpdatePayload {
    #[serde(default)]
    default_provider: Option<ProviderId>,
    #[serde(default)]
    code_provider: Option<ProviderId>,
    #[serde(default)]
    reasoning_provider: Option<ProviderId>,
    #[serde(default)]
    local_provider: Option<ProviderId>,
}

#[derive(Clone, Debug, Deserialize)]
struct RuleUpdatePayload {
    name: String,
    prefer: ProviderId,
    #[serde(default)]
    task: Option<TaskHint>,
    #[serde(default)]
    private: Option<bool>,
    #[serde(default)]
    long_context: Option<bool>,
    #[serde(default)]
    repo: Option<bool>,
    #[serde(default)]
    max_input_tokens: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
struct RuleRemovePayload {
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CreateDirectoryPayload {
    parent: PathBuf,
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct FavoritePathPayload {
    path: PathBuf,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct FavoriteRemovePayload {
    path: String,
}

struct ServerRuntime<'a> {
    health_report_override: Option<&'a [ProviderHealth]>,
    queue: &'a ExecutionQueue,
    log_path: Option<&'a Path>,
    config_path: Option<&'a Path>,
    headers: &'a [(&'a str, &'a str)],
    directory_picker: &'a dyn Fn() -> Result<Option<PathBuf>, String>,
}

pub fn handle_server_request(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
) -> ServerResponse {
    handle_server_request_with_headers_and_runner(
        config,
        method,
        path,
        body,
        &[],
        |provider, prompt, cwd| {
            run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
        },
    )
}

pub fn handle_server_request_with_headers(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> ServerResponse {
    handle_server_request_with_headers_and_runner(
        config,
        method,
        path,
        body,
        headers,
        |provider, prompt, cwd| {
            run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
        },
    )
}

pub fn handle_server_request_with_runner<F>(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    runner: F,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    handle_server_request_with_headers_and_runner(config, method, path, body, &[], runner)
}

pub fn handle_server_request_with_headers_and_runner<F>(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
    runner: F,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let mut config = config.clone();
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override: None,
        queue: &queue,
        log_path: None,
        config_path: None,
        headers,
        directory_picker: &native_directory_picker,
    };
    handle_server_request_with_queue_and_report(&mut config, method, path, body, runner, runtime)
}

pub fn handle_server_request_with_config_path(
    config: &mut RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    handle_server_request_with_config_path_and_runner(
        config,
        method,
        path,
        body,
        config_path,
        |provider, prompt, cwd| {
            run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
        },
    )
}

pub fn handle_server_request_with_config_path_and_headers(
    config: &mut RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    config_path: Option<&Path>,
    headers: &[(&str, &str)],
) -> ServerResponse {
    handle_server_request_with_config_path_headers_and_runner(
        config,
        method,
        path,
        body,
        config_path,
        headers,
        |provider, prompt, cwd| {
            run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
        },
    )
}

pub fn handle_server_request_with_config_path_and_runner<F>(
    config: &mut RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    config_path: Option<&Path>,
    runner: F,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override: None,
        queue: &queue,
        log_path: None,
        config_path,
        headers: &[],
        directory_picker: &native_directory_picker,
    };
    handle_server_request_with_queue_and_report(config, method, path, body, runner, runtime)
}

pub fn handle_server_request_with_config_path_headers_and_runner<F>(
    config: &mut RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    config_path: Option<&Path>,
    headers: &[(&str, &str)],
    runner: F,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override: None,
        queue: &queue,
        log_path: None,
        config_path,
        headers,
        directory_picker: &native_directory_picker,
    };
    handle_server_request_with_queue_and_report(config, method, path, body, runner, runtime)
}

pub fn handle_server_request_with_runner_and_report<F>(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    runner: F,
    health_report_override: Option<&[ProviderHealth]>,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let mut config = config.clone();
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override,
        queue: &queue,
        log_path: None,
        config_path: None,
        headers: &[],
        directory_picker: &native_directory_picker,
    };
    handle_server_request_with_queue_and_report(&mut config, method, path, body, runner, runtime)
}

pub fn handle_server_request_with_headers_runner_and_report<F>(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
    runner: F,
    health_report_override: Option<&[ProviderHealth]>,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let mut config = config.clone();
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override,
        queue: &queue,
        log_path: None,
        config_path: None,
        headers,
        directory_picker: &native_directory_picker,
    };
    handle_server_request_with_queue_and_report(&mut config, method, path, body, runner, runtime)
}

pub fn handle_server_request_with_directory_picker<P>(
    config: &RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    picker: P,
) -> ServerResponse
where
    P: Fn() -> Result<Option<PathBuf>, String>,
{
    let mut config = config.clone();
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override: None,
        queue: &queue,
        log_path: None,
        config_path: None,
        headers: &[],
        directory_picker: &picker,
    };
    handle_server_request_with_queue_and_report(
        &mut config,
        method,
        path,
        body,
        |provider, prompt, cwd| {
            run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
        },
        runtime,
    )
}

fn handle_server_request_with_queue_and_report<F>(
    config: &mut RouterConfig,
    method: &str,
    path: &str,
    body: &str,
    runner: F,
    runtime: ServerRuntime<'_>,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return json_response(
            413,
            json!({ "error": "request_too_large", "max_bytes": MAX_REQUEST_BODY_BYTES }),
        );
    }

    let (route_path, query) = split_path_and_query(path);

    if !authorized(config, runtime.headers, route_path) {
        return json_response(401, json!({ "error": "unauthorized" }));
    }

    let owned_report;
    let report = match runtime.health_report_override {
        Some(report) => report,
        None => {
            owned_report = health_report(config);
            &owned_report
        }
    };

    if method == "GET" {
        return match route_path {
            "/" => html_response(200, GUI_HTML),
            "/config" => config_response(config),
            "/health" => health_response(report),
            "/history" => history_response(runtime.log_path),
            "/queue" => json_response(200, json!({ "jobs": queue_views(runtime.queue) })),
            "/favorites" => favorites_response(config),
            "/metrics" => metrics_response(runtime.log_path),
            "/spend" => metrics_response(runtime.log_path),
            "/budget" => json_response(200, json!(config.budget)),
            "/fs" => fs_response(query),
            _ => json_response(404, json!({ "error": "not_found" })),
        };
    }

    if method != "POST" {
        return json_response(405, json!({ "error": "method_not_allowed" }));
    }

    match route_path {
        "/route" => route_response(config, body, report),
        "/run" => run_response(config, body, runner, report),
        "/queue" => queue_response(config, body, runner, report, runtime.queue),
        "/feedback" => feedback_response(body, runtime.log_path),
        "/fs/create-directory" => create_directory_response(body),
        "/fs/pick-directory" => pick_directory_response(runtime.directory_picker),
        "/favorites" => favorite_update_response(config, body, runtime.config_path),
        "/favorites/remove" => favorite_remove_response(config, body, runtime.config_path),
        "/provider-test" => provider_test_response(body, report),
        "/config/validate" => config_validate_response(body),
        "/config" => config_save_response(config, body, runtime.config_path),
        "/config/provider" => provider_update_response(config, body, runtime.config_path),
        "/config/settings" => settings_update_response(config, body, runtime.config_path),
        "/config/rule" => rule_update_response(config, body, runtime.config_path),
        "/config/rule/remove" => rule_remove_response(config, body, runtime.config_path),
        "/config/profile" => profile_update_response(config, body, runtime.config_path),
        "/v1/chat/completions" => openai_chat_completion_response(config, body, runner, report),
        _ => json_response(404, json!({ "error": "not_found" })),
    }
}

pub fn serve<A>(config: RouterConfig, address: A) -> anyhow::Result<()>
where
    A: ToSocketAddrs,
{
    serve_with_log(config, address, None)
}

pub fn serve_with_log<A>(
    config: RouterConfig,
    address: A,
    log_path: Option<PathBuf>,
) -> anyhow::Result<()>
where
    A: ToSocketAddrs,
{
    serve_with_log_and_config_path(config, address, log_path, None)
}

pub fn serve_with_log_and_config_path<A>(
    mut config: RouterConfig,
    address: A,
    log_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> anyhow::Result<()>
where
    A: ToSocketAddrs,
{
    let server = Server::http(address).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let queue = ExecutionQueue::default();

    for mut request in server.incoming_requests() {
        let mut body = String::new();
        let mut reader = request
            .as_reader()
            .take(u64::try_from(MAX_REQUEST_BODY_BYTES + 1).unwrap_or(u64::MAX));
        reader.read_to_string(&mut body)?;
        let path = request.url().to_string();
        let headers = request
            .headers()
            .iter()
            .map(|header| (header.field.to_string(), header.value.as_str().to_string()))
            .collect::<Vec<_>>();
        let header_refs = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let runtime = ServerRuntime {
            health_report_override: None,
            queue: &queue,
            log_path: log_path.as_deref(),
            config_path: config_path.as_deref(),
            headers: &header_refs,
            directory_picker: &native_directory_picker,
        };
        let response = handle_server_request_with_queue_and_report(
            &mut config,
            request.method().as_str(),
            &path,
            &body,
            |provider, prompt, cwd| {
                run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
            },
            runtime,
        );
        if let Some(log_path) = log_path.as_deref() {
            log_response(log_path, &path, &body, &response);
        }
        request.respond(to_tiny_response(response))?;
    }

    Ok(())
}

fn route_response(config: &RouterConfig, body: &str, report: &[ProviderHealth]) -> ServerResponse {
    let payload = match parse_payload(body) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let request = build_route_request(&payload);
    let profiled = config_for_payload(config, &payload);
    let filtered = availability_filtered_config(&profiled, report);
    match Router::new(filtered).route(request) {
        Ok(decision) => json_response(200, json!(decision)),
        Err(error) => json_response(
            422,
            json!({ "error": "route_failed", "message": error.to_string() }),
        ),
    }
}

fn run_response<F>(
    config: &RouterConfig,
    body: &str,
    runner: F,
    report: &[ProviderHealth],
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let payload = match parse_payload(body) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let request = build_route_request(&payload);
    let profiled = config_for_payload(config, &payload);
    let filtered = availability_filtered_config(&profiled, report);
    let decision = match Router::new(filtered).route(request) {
        Ok(decision) => decision,
        Err(error) => {
            return json_response(
                422,
                json!({ "error": "route_failed", "message": error.to_string() }),
            );
        }
    };
    let provider = match profiled.provider(decision.provider) {
        Some(provider) => provider,
        None => {
            return json_response(
                500,
                json!({ "error": "provider_missing", "provider": decision.provider }),
            );
        }
    };
    match runner(provider, &payload.prompt, payload.cwd.as_deref()) {
        Ok(output) => json_response(200, json!(RunResponse { decision, output })),
        Err(message) => json_response(
            502,
            json!({ "error": "provider_failed", "message": message }),
        ),
    }
}

fn queue_response<F>(
    config: &RouterConfig,
    body: &str,
    runner: F,
    report: &[ProviderHealth],
    queue: &ExecutionQueue,
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let payload = match parse_payload(body) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let prompt = payload.prompt.clone();
    let id = queue.submit(prompt, |queued_prompt| {
        let request = build_route_request(&payload);
        let profiled = config_for_payload(config, &payload);
        let filtered = availability_filtered_config(&profiled, report);
        let decision = Router::new(filtered)
            .route(request)
            .map_err(|error| error.to_string())?;
        let provider = profiled
            .provider(decision.provider)
            .ok_or_else(|| format!("{} is not configured", decision.provider))?;
        runner(provider, queued_prompt, payload.cwd.as_deref())
    });
    let Some(job) = queue.get(id) else {
        return json_response(500, json!({ "error": "queue_missing_job" }));
    };
    json_response(202, json!(queue_view(&job)))
}

fn metrics_response(log_path: Option<&Path>) -> ServerResponse {
    let metrics = match log_path {
        Some(path) => load_metrics(path).unwrap_or_else(|_| metrics_from_logs(&[])),
        None => metrics_from_logs(&[]),
    };
    json_response(200, json!(metrics))
}

fn history_response(log_path: Option<&Path>) -> ServerResponse {
    let Some(path) = log_path else {
        return json_response(200, json!({ "entries": [] }));
    };
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return json_response(200, json!({ "entries": [] }));
        }
        Err(error) => {
            return json_response(
                500,
                json!({ "error": "history_unreadable", "message": error.to_string() }),
            );
        }
    };
    let mut entries = body
        .lines()
        .filter_map(|line| serde_json::from_str::<RequestLogEntry>(line).ok())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_unix_ms));
    entries.truncate(50);
    json_response(200, json!({ "entries": entries }))
}

fn favorites_response(config: &RouterConfig) -> ServerResponse {
    json_response(200, json!({ "favorites": config.favorites }))
}

fn config_response(config: &RouterConfig) -> ServerResponse {
    match toml::to_string_pretty(config) {
        Ok(contents) => json_response(
            200,
            json!({
                "config": config,
                "toml": contents,
            }),
        ),
        Err(error) => json_response(
            500,
            json!({ "error": "config_serialize_failed", "message": error.to_string() }),
        ),
    }
}

fn config_validate_response(body: &str) -> ServerResponse {
    let payload = match parse_config_toml_payload(body) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    match parse_config_toml(&payload.toml) {
        Ok(_) => json_response(200, json!({ "valid": true })),
        Err(response) => response,
    }
}

fn config_save_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match parse_config_toml_payload(body) {
        Ok(payload) => payload,
        Err(response) => return response,
    };
    let parsed = match parse_config_toml(&payload.toml) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    match persist_config(config, parsed, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn provider_test_response(body: &str, report: &[ProviderHealth]) -> ServerResponse {
    let payload = match serde_json::from_str::<ProviderTestPayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    match report
        .iter()
        .find(|provider| provider.provider == payload.provider)
    {
        Some(provider) => json_response(200, json!(sanitize_health(provider.clone()))),
        None => json_response(
            404,
            json!({ "error": "provider_not_found", "provider": payload.provider }),
        ),
    }
}

fn provider_update_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<ProviderUpdatePayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let mut next = config.clone();
    let Some(provider) = next
        .providers
        .iter_mut()
        .find(|provider| provider.id == payload.provider)
    else {
        return json_response(
            404,
            json!({ "error": "provider_not_found", "provider": payload.provider }),
        );
    };
    if let Some(enabled) = payload.enabled {
        provider.enabled = enabled;
    }
    if let Some(model) = payload.model.filter(|model| !model.trim().is_empty()) {
        provider.model = model;
    }
    if payload.endpoint_url.is_some() {
        provider.endpoint_url = payload
            .endpoint_url
            .and_then(|value| (!value.trim().is_empty()).then_some(value));
    }
    if let Err(error) = next.validate() {
        return json_response(
            422,
            json!({ "error": "config_invalid", "message": error.to_string() }),
        );
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn settings_update_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let raw = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let payload = match serde_json::from_value::<SettingsUpdatePayload>(raw.clone()) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let mut next = config.clone();
    if let Some(provider) = payload.default_provider {
        next.routing.default_provider = provider;
    }
    if let Some(provider) = payload.code_provider {
        next.routing.code_provider = provider;
    }
    if let Some(provider) = payload.reasoning_provider {
        next.routing.reasoning_provider = provider;
    }
    if let Some(provider) = payload.local_provider {
        next.routing.local_provider = provider;
    }
    if let Some(value) = raw.get("monthly_api_budget_cents").cloned() {
        let Some(budget) = parse_optional_non_negative_f64(value) else {
            return json_response(
                422,
                json!({
                    "error": "settings_invalid",
                    "message": "monthly_api_budget_cents must be a non-negative number or null",
                }),
            );
        };
        next.budget.monthly_api_budget_cents = budget;
    }
    if let Some(value) = raw.get("auth_token").cloned() {
        let Some(auth_token) = parse_optional_string(value) else {
            return json_response(
                422,
                json!({
                    "error": "settings_invalid",
                    "message": "auth_token must be a string or null",
                }),
            );
        };
        next.server.auth_token = auth_token;
    }
    if let Err(error) = next.validate() {
        return json_response(
            422,
            json!({ "error": "config_invalid", "message": error.to_string() }),
        );
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn rule_update_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<RuleUpdatePayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return json_response(
            422,
            json!({ "error": "rule_invalid", "message": "name is required" }),
        );
    }
    let rule = RouteRule {
        name,
        prefer: payload.prefer,
        when: RouteMatch {
            task: payload.task,
            private: payload.private,
            long_context: payload.long_context,
            repo: payload.repo,
            max_input_tokens: payload.max_input_tokens,
        },
    };
    let mut next = config.clone();
    if let Some(existing) = next
        .rules
        .iter_mut()
        .find(|existing| existing.name == rule.name)
    {
        *existing = rule;
    } else {
        next.rules.push(rule);
    }
    next.rules.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    if let Err(error) = next.validate() {
        return json_response(
            422,
            json!({ "error": "config_invalid", "message": error.to_string() }),
        );
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn rule_remove_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<RuleRemovePayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let name = payload.name.trim();
    if name.is_empty() {
        return json_response(
            422,
            json!({ "error": "rule_invalid", "message": "name is required" }),
        );
    }
    let mut next = config.clone();
    let before = next.rules.len();
    next.rules.retain(|rule| rule.name != name);
    if next.rules.len() == before {
        return json_response(404, json!({ "error": "rule_not_found" }));
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn profile_update_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<ProfileUpdatePayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    if payload.name.trim().is_empty() || payload.path_contains.trim().is_empty() {
        return json_response(
            422,
            json!({ "error": "profile_invalid", "message": "name and path_contains are required" }),
        );
    }
    let mut next = config.clone();
    let profile = ProjectProfile {
        name: payload.name,
        path_contains: payload.path_contains,
        default_provider: payload.default_provider,
        code_provider: payload.code_provider,
        reasoning_provider: payload.reasoning_provider,
        local_provider: payload.local_provider,
    };
    if let Some(existing) = next
        .profiles
        .iter_mut()
        .find(|existing| existing.name == profile.name)
    {
        *existing = profile;
    } else {
        next.profiles.push(profile);
    }
    if let Err(error) = next.validate() {
        return json_response(
            422,
            json!({ "error": "config_invalid", "message": error.to_string() }),
        );
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn favorite_update_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<FavoritePathPayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let path = match canonical_directory(&payload.path) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let path = path.display().to_string();
    let name = payload
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| favorite_name_for_path(Path::new(&path)));
    let favorite = PathFavorite {
        name,
        path: path.clone(),
    };
    let mut next = config.clone();
    if let Some(existing) = next
        .favorites
        .iter_mut()
        .find(|existing| existing.path == favorite.path)
    {
        *existing = favorite;
    } else {
        next.favorites.push(favorite);
    }
    next.favorites.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn favorite_remove_response(
    config: &mut RouterConfig,
    body: &str,
    config_path: Option<&Path>,
) -> ServerResponse {
    let payload = match serde_json::from_str::<FavoriteRemovePayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let requested = payload.path.trim();
    if requested.is_empty() {
        return json_response(
            422,
            json!({ "error": "favorite_invalid", "message": "path is required" }),
        );
    }
    let expanded = expand_home_path(Path::new(requested));
    let canonical = fs::canonicalize(&expanded)
        .ok()
        .map(|path| path.display().to_string());
    let mut next = config.clone();
    let before = next.favorites.len();
    next.favorites.retain(|favorite| {
        favorite.path != requested && Some(&favorite.path) != canonical.as_ref()
    });
    if next.favorites.len() == before {
        return json_response(404, json!({ "error": "favorite_not_found" }));
    }
    match persist_config(config, next, config_path) {
        Ok(save) => json_response(200, save),
        Err(response) => response,
    }
}

fn create_directory_response(body: &str) -> ServerResponse {
    let payload = match serde_json::from_str::<CreateDirectoryPayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };
    let parent = match canonical_directory(&payload.parent) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let name = payload.name.trim();
    if !valid_new_directory_name(name) {
        return json_response(
            422,
            json!({ "error": "invalid_directory_name", "message": "Use a single folder name without path separators." }),
        );
    }
    let path = parent.join(name);
    if path.exists() {
        return json_response(
            409,
            json!({ "error": "path_exists", "path": path.display().to_string() }),
        );
    }
    if let Err(error) = fs::create_dir(&path) {
        return json_response(
            500,
            json!({ "error": "directory_create_failed", "message": error.to_string() }),
        );
    }
    json_response(
        201,
        json!({
            "created": true,
            "name": name,
            "parent": parent.display().to_string(),
            "path": path.display().to_string(),
        }),
    )
}

fn fs_response(query: Option<&str>) -> ServerResponse {
    let requested = query_value(query, "path")
        .filter(|path| !path.trim().is_empty())
        .map_or_else(
            || env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );
    let requested = expand_home_path(&requested);
    let path = match fs::canonicalize(&requested) {
        Ok(path) => path,
        Err(error) => {
            return json_response(
                404,
                json!({ "error": "path_not_found", "message": error.to_string() }),
            );
        }
    };

    if !path.is_dir() {
        return json_response(
            422,
            json!({ "error": "not_a_directory", "path": path.display().to_string() }),
        );
    }

    let mut entries = match fs::read_dir(&path) {
        Ok(entries) => entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let file_type = entry.file_type().ok()?;
                let kind = if file_type.is_dir() {
                    "directory"
                } else if file_type.is_file() {
                    "file"
                } else {
                    return None;
                };
                Some(json!({
                    "kind": kind,
                    "name": entry.file_name().to_string_lossy(),
                    "path": entry.path().display().to_string(),
                }))
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            return json_response(
                403,
                json!({ "error": "directory_unreadable", "message": error.to_string() }),
            );
        }
    };
    entries.sort_by(|left, right| {
        let left_kind = left["kind"].as_str().unwrap_or_default();
        let right_kind = right["kind"].as_str().unwrap_or_default();
        left_kind.cmp(right_kind).then_with(|| {
            left["name"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .cmp(
                    &right["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                )
        })
    });

    json_response(
        200,
        json!({
            "entries": entries,
            "parent": path.parent().map(|parent| parent.display().to_string()),
            "path": path.display().to_string(),
        }),
    )
}

fn pick_directory_response(picker: &dyn Fn() -> Result<Option<PathBuf>, String>) -> ServerResponse {
    let selected = match picker() {
        Ok(Some(path)) => path,
        Ok(None) => return json_response(200, json!({ "cancelled": true, "path": null })),
        Err(message) => {
            return json_response(
                501,
                json!({ "error": "directory_picker_unavailable", "message": message }),
            );
        }
    };
    let selected = expand_home_path(&selected);
    let path = match fs::canonicalize(&selected) {
        Ok(path) => path,
        Err(error) => {
            return json_response(
                422,
                json!({ "error": "path_not_found", "message": error.to_string() }),
            );
        }
    };
    if !path.is_dir() {
        return json_response(
            422,
            json!({ "error": "not_a_directory", "path": path.display().to_string() }),
        );
    }
    json_response(
        200,
        json!({
            "cancelled": false,
            "path": path.display().to_string(),
        }),
    )
}

fn feedback_response(body: &str, log_path: Option<&Path>) -> ServerResponse {
    let entry = match serde_json::from_str::<FeedbackEntry>(body) {
        Ok(entry) => entry,
        Err(error) => {
            return json_response(
                400,
                json!({ "error": "invalid_json", "message": error.to_string() }),
            );
        }
    };

    if let Some(log_path) = log_path {
        let feedback_path = log_path.with_file_name("feedback.jsonl");
        if let Err(error) = append_feedback(&feedback_path, &entry) {
            return json_response(
                500,
                json!({ "error": "feedback_failed", "message": error.to_string() }),
            );
        }
    }

    json_response(202, json!(entry))
}

fn openai_chat_completion_response<F>(
    config: &RouterConfig,
    body: &str,
    runner: F,
    report: &[ProviderHealth],
) -> ServerResponse
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let payload = match serde_json::from_str::<OpenAiChatPayload>(body) {
        Ok(payload) => payload,
        Err(error) => {
            return json_response(
                400,
                json!({
                    "error": {
                        "code": "invalid_json",
                        "message": error.to_string()
                    }
                }),
            );
        }
    };

    if payload.stream {
        return json_response(
            400,
            json!({
                "error": {
                    "code": "streaming_not_supported",
                    "message": "Modelrouter proxy currently supports non-streaming chat completions only."
                }
            }),
        );
    }

    let prompt = openai_messages_to_prompt(&payload.messages);
    if prompt.trim().is_empty() {
        return json_response(
            400,
            json!({
                "error": {
                    "code": "empty_prompt",
                    "message": "At least one chat message with content is required."
                }
            }),
        );
    }

    let request = RouteRequest::new(prompt.clone());
    let filtered = availability_filtered_config(config, report);
    let decision = match Router::new(filtered).route(request) {
        Ok(decision) => decision,
        Err(error) => {
            return json_response(
                422,
                json!({
                    "error": {
                        "code": "route_failed",
                        "message": error.to_string()
                    }
                }),
            );
        }
    };
    let provider = match config.provider(decision.provider) {
        Some(provider) => provider,
        None => {
            return json_response(
                500,
                json!({
                    "error": {
                        "code": "provider_missing",
                        "message": format!("{} is not configured", decision.provider)
                    }
                }),
            );
        }
    };
    let output = match runner(provider, &prompt, None) {
        Ok(output) => output,
        Err(message) => {
            return json_response(
                502,
                json!({
                    "error": {
                        "code": "provider_failed",
                        "message": message
                    }
                }),
            );
        }
    };

    json_response(
        200,
        json!({
            "id": "chatcmpl-modelrouter",
            "object": "chat.completion",
            "created": 0,
            "model": payload.model.unwrap_or_else(|| "modelrouter".to_string()),
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": output
                    },
                    "finish_reason": "stop"
                }
            ],
            "modelrouter": decision
        }),
    )
}

fn openai_messages_to_prompt(messages: &[OpenAiMessage]) -> String {
    messages
        .iter()
        .filter(|message| !message.content.trim().is_empty())
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_payload(body: &str) -> Result<RoutePayload, ServerResponse> {
    serde_json::from_str::<RoutePayload>(body).map_err(|error| {
        json_response(
            400,
            json!({ "error": "invalid_json", "message": error.to_string() }),
        )
    })
}

fn parse_config_toml_payload(body: &str) -> Result<ConfigTomlPayload, ServerResponse> {
    serde_json::from_str::<ConfigTomlPayload>(body).map_err(|error| {
        json_response(
            400,
            json!({ "error": "invalid_json", "message": error.to_string() }),
        )
    })
}

fn parse_config_toml(contents: &str) -> Result<RouterConfig, ServerResponse> {
    let config = match toml::from_str::<RouterConfig>(contents) {
        Ok(config) => config,
        Err(error) => {
            return Err(json_response(
                422,
                json!({
                    "error": "config_invalid",
                    "message": error.to_string(),
                    "valid": false,
                }),
            ));
        }
    };
    if let Err(error) = config.validate() {
        return Err(json_response(
            422,
            json!({
                "error": "config_invalid",
                "message": error.to_string(),
                "valid": false,
            }),
        ));
    }
    Ok(config)
}

fn parse_optional_non_negative_f64(value: Value) -> Option<Option<f64>> {
    match value {
        Value::Null => Some(None),
        Value::Number(number) => number.as_f64().filter(|value| *value >= 0.0).map(Some),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                Some(None)
            } else {
                text.parse::<f64>()
                    .ok()
                    .filter(|value| *value >= 0.0)
                    .map(Some)
            }
        }
        _ => None,
    }
}

fn parse_optional_string(value: Value) -> Option<Option<String>> {
    match value {
        Value::Null => Some(None),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                Some(None)
            } else {
                Some(Some(text.to_string()))
            }
        }
        _ => None,
    }
}

fn persist_config(
    config: &mut RouterConfig,
    next: RouterConfig,
    config_path: Option<&Path>,
) -> Result<Value, ServerResponse> {
    let Some(path) = config_path else {
        return Err(json_response(
            409,
            json!({ "error": "config_path_missing", "message": "Start the daemon with --config to save config changes." }),
        ));
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && let Err(error) = fs::create_dir_all(parent)
    {
        return Err(json_response(
            500,
            json!({ "error": "config_save_failed", "message": error.to_string() }),
        ));
    }
    let backup_path = backup_path(path);
    if path.is_file()
        && let Err(error) = fs::copy(path, &backup_path)
    {
        return Err(json_response(
            500,
            json!({ "error": "config_backup_failed", "message": error.to_string() }),
        ));
    }
    let contents = match toml::to_string_pretty(&next) {
        Ok(contents) => contents,
        Err(error) => {
            return Err(json_response(
                500,
                json!({ "error": "config_serialize_failed", "message": error.to_string() }),
            ));
        }
    };
    let temp_path = temp_config_path(path);
    if let Err(error) = fs::write(&temp_path, contents) {
        return Err(json_response(
            500,
            json!({ "error": "config_save_failed", "message": error.to_string() }),
        ));
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(json_response(
            500,
            json!({ "error": "config_save_failed", "message": error.to_string() }),
        ));
    }
    *config = next;
    Ok(json!({
        "backup_path": backup_path.display().to_string(),
        "config": config,
        "path": path.display().to_string(),
        "reloaded": true,
        "saved": true,
        "toml": toml::to_string_pretty(config).unwrap_or_default(),
    }))
}

fn backup_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(|| "bak".to_string(), |extension| format!("{extension}.bak"));
    path.with_extension(extension)
}

fn temp_config_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(|| "tmp".to_string(), |extension| format!("{extension}.tmp"));
    path.with_extension(extension)
}

fn build_route_request(payload: &RoutePayload) -> RouteRequest {
    let mut request = RouteRequest::new(payload.prompt.clone());
    if let Some(provider) = payload.prefer {
        request = request.prefer(provider);
    }
    if let Some(max_cost_cents) = payload.max_cost_cents {
        request = request.with_max_cost_cents(max_cost_cents);
    }
    if let Some(tokens) = payload.input_tokens {
        request = request.with_estimated_input_tokens(tokens);
    }
    if let Some(tokens) = payload.output_tokens {
        request = request.with_estimated_output_tokens(tokens);
    }
    if let Some(hint) = payload.hint {
        request = request.with_hint(hint);
    }
    request
}

fn config_for_payload(config: &RouterConfig, payload: &RoutePayload) -> RouterConfig {
    payload
        .cwd
        .as_deref()
        .map_or_else(|| config.clone(), |cwd| apply_project_profile(config, cwd))
}

fn json_response(status: u16, body: Value) -> ServerResponse {
    ServerResponse {
        status,
        body,
        content_type: "application/json",
    }
}

fn html_response(status: u16, html: &str) -> ServerResponse {
    ServerResponse {
        status,
        body: json!({ "html": html }),
        content_type: "text/html; charset=utf-8",
    }
}

fn health_response(report: &[ProviderHealth]) -> ServerResponse {
    let sanitized = report
        .iter()
        .cloned()
        .map(sanitize_health)
        .collect::<Vec<_>>();
    json_response(200, json!({ "providers": sanitized }))
}

fn sanitize_health(mut provider: ProviderHealth) -> ProviderHealth {
    if provider.check.starts_with("http:") {
        provider.check = "http:[redacted]".to_string();
    }
    provider
}

fn queue_views(queue: &ExecutionQueue) -> Vec<serde_json::Value> {
    queue.list().iter().map(queue_view).collect()
}

fn queue_view(job: &crate::QueueJob) -> serde_json::Value {
    json!({
        "id": job.id,
        "status": job.status,
    })
}

fn authorized(config: &RouterConfig, headers: &[(&str, &str)], path: &str) -> bool {
    let Some(token) = config.server.auth_token.as_deref() else {
        return true;
    };
    if matches!(path, "/" | "/health") {
        return true;
    }
    let expected = format!("Bearer {token}");
    headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("authorization") && value.trim() == expected)
}

fn split_path_and_query(path: &str) -> (&str, Option<&str>) {
    path.split_once('?')
        .map_or((path, None), |(path, query)| (path, Some(query)))
}

fn query_value(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (percent_decode(name) == key).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    decoded.push((high << 4) | low);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn expand_home_path(path: &Path) -> PathBuf {
    let Some(raw_path) = path.to_str() else {
        return path.to_path_buf();
    };
    if raw_path == "~" {
        return env::var("HOME").map_or_else(|_| path.to_path_buf(), PathBuf::from);
    }
    if let Some(rest) = raw_path.strip_prefix("~/") {
        return env::var("HOME").map_or_else(
            |_| path.to_path_buf(),
            |home| PathBuf::from(home).join(rest),
        );
    }
    path.to_path_buf()
}

fn canonical_directory(path: &Path) -> Result<PathBuf, ServerResponse> {
    let requested = expand_home_path(path);
    let path = match fs::canonicalize(&requested) {
        Ok(path) => path,
        Err(error) => {
            return Err(json_response(
                404,
                json!({ "error": "path_not_found", "message": error.to_string() }),
            ));
        }
    };
    if !path.is_dir() {
        return Err(json_response(
            422,
            json!({ "error": "not_a_directory", "path": path.display().to_string() }),
        ));
    }
    Ok(path)
}

fn favorite_name_for_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Favorite")
        .to_string()
}

fn valid_new_directory_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !Path::new(name).is_absolute()
}

fn native_directory_picker() -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("osascript");
        command.args([
            "-e",
            r#"POSIX path of (choose folder with prompt "Choose a directory for Modelrouter")"#,
        ]);
        run_directory_picker_command(&mut command)
    }

    #[cfg(target_os = "linux")]
    {
        if env::var_os("DISPLAY").is_none() && env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err(
                "No desktop session is available for a native directory picker.".to_string(),
            );
        }
        let mut command = Command::new("zenity");
        command.args([
            "--file-selection",
            "--directory",
            "--title=Choose a directory for Modelrouter",
        ]);
        run_directory_picker_command(&mut command)
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'Choose a directory for Modelrouter'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  Write-Output $dialog.SelectedPath
}
"#;
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-Command", script]);
        run_directory_picker_command(&mut command)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("Native directory picker is not supported on this platform.".to_string())
    }
}

fn run_directory_picker_command(command: &mut Command) -> Result<Option<PathBuf>, String> {
    let output = command
        .output()
        .map_err(|error| format!("Unable to open native directory picker: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let selected = stdout.trim();
    if output.status.success() {
        return if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(selected)))
        };
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if selected.is_empty() && (stderr.is_empty() || stderr.to_ascii_lowercase().contains("cancel"))
    {
        Ok(None)
    } else {
        Err(format!("Native directory picker failed: {stderr}"))
    }
}

fn to_tiny_response(response: ServerResponse) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = if response.content_type.starts_with("text/html") {
        response
            .body
            .get("html")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        response.body.to_string()
    };
    let mut tiny_response =
        Response::from_string(body).with_status_code(StatusCode(response.status));
    if let Ok(header) = Header::from_bytes("Content-Type", response.content_type) {
        tiny_response.add_header(header);
    }
    tiny_response
}

fn log_response(log_path: &Path, path: &str, request_body: &str, response: &ServerResponse) {
    if !matches!(path, "/route" | "/run") {
        return;
    }
    let Ok(payload) = serde_json::from_str::<RoutePayload>(request_body) else {
        return;
    };
    let decision_value = if path == "/run" {
        response.body.get("decision")
    } else {
        Some(&response.body)
    };
    let Some(decision_value) = decision_value else {
        return;
    };
    let Ok(decision) = serde_json::from_value::<RouteDecision>(decision_value.clone()) else {
        return;
    };
    let entry = RequestLogEntry::from_decision(
        path.trim_start_matches('/'),
        &payload.prompt,
        &decision,
        0,
        response.status < 400,
        (response.status >= 400).then(|| response.body.to_string()),
    );
    let _ = append_request_log(log_path, &entry);
}
