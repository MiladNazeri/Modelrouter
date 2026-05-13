use std::{
    io::Read,
    net::ToSocketAddrs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::{
    ExecutionQueue, FeedbackEntry, GUI_HTML, ProviderConfig, ProviderHealth, ProviderId,
    RequestLogEntry, RouteDecision, RouteRequest, Router, RouterConfig, TaskHint, append_feedback,
    append_request_log, apply_project_profile, availability_filtered_config, health_report,
    load_metrics, metrics_from_logs, run_provider,
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

struct ServerRuntime<'a> {
    health_report_override: Option<&'a [ProviderHealth]>,
    queue: &'a ExecutionQueue,
    log_path: Option<&'a Path>,
    headers: &'a [(&'a str, &'a str)],
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
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override: None,
        queue: &queue,
        log_path: None,
        headers,
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
    let queue = ExecutionQueue::default();
    let runtime = ServerRuntime {
        health_report_override,
        queue: &queue,
        log_path: None,
        headers: &[],
    };
    handle_server_request_with_queue_and_report(config, method, path, body, runner, runtime)
}

fn handle_server_request_with_queue_and_report<F>(
    config: &RouterConfig,
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

    if !authorized(config, runtime.headers, path) {
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
        return match path {
            "/" => html_response(200, GUI_HTML),
            "/health" => health_response(report),
            "/queue" => json_response(200, json!({ "jobs": queue_views(runtime.queue) })),
            "/metrics" => metrics_response(runtime.log_path),
            "/spend" => metrics_response(runtime.log_path),
            "/budget" => json_response(200, json!(config.budget)),
            _ => json_response(404, json!({ "error": "not_found" })),
        };
    }

    if method != "POST" {
        return json_response(405, json!({ "error": "method_not_allowed" }));
    }

    match path {
        "/route" => route_response(config, body, report),
        "/run" => run_response(config, body, runner, report),
        "/queue" => queue_response(config, body, runner, report, runtime.queue),
        "/feedback" => feedback_response(body, runtime.log_path),
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
    let server = Server::http(address).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let queue = ExecutionQueue::default();

    for mut request in server.incoming_requests() {
        let mut body = String::new();
        let mut reader = request
            .as_reader()
            .take(u64::try_from(MAX_REQUEST_BODY_BYTES + 1).unwrap_or(u64::MAX));
        reader.read_to_string(&mut body)?;
        let path = request
            .url()
            .split_once('?')
            .map_or_else(|| request.url(), |(path, _query)| path);
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
            headers: &header_refs,
        };
        let response = handle_server_request_with_queue_and_report(
            &config,
            request.method().as_str(),
            path,
            &body,
            |provider, prompt, cwd| {
                run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
            },
            runtime,
        );
        if let Some(log_path) = log_path.as_deref() {
            log_response(log_path, path, &body, &response);
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
        .map(|mut provider| {
            if provider.check.starts_with("http:") {
                provider.check = "http:[redacted]".to_string();
            }
            provider
        })
        .collect::<Vec<_>>();
    json_response(200, json!({ "providers": sanitized }))
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
