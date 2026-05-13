use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use serde_json::{Value, json};

use crate::{ProviderConfig, RouteRequest, Router, RouterConfig, run_provider};

pub fn handle_mcp_message(config: &RouterConfig, message: &str) -> Option<Value> {
    handle_mcp_message_with_runner(config, message, |provider, prompt, cwd| {
        run_provider(provider, prompt, cwd).map_err(|error| error.to_string())
    })
}

pub fn handle_mcp_message_with_runner<F>(
    config: &RouterConfig,
    message: &str,
    runner: F,
) -> Option<Value>
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let value = match serde_json::from_str::<Value>(message) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_response(Value::Null, -32700, &error.to_string()));
        }
    };
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return Some(error_response(id, -32600, "missing method"));
    };

    match method {
        "notifications/initialized" => None,
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "modelrouter", "version": env!("CARGO_PKG_VERSION") }
            }
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    route_tool(),
                    run_tool(),
                    broker_tool()
                ]
            }
        })),
        "tools/call" => Some(call_tool(config, id, &value, runner)),
        _ => Some(error_response(id, -32601, "method not found")),
    }
}

pub fn serve_mcp(config: RouterConfig) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if let Some(response) = handle_mcp_message(&config, &line) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn call_tool<F>(config: &RouterConfig, id: Value, message: &Value, runner: F) -> Value
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let prompt = arguments
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if prompt.trim().is_empty() {
        return error_response(id, -32602, "prompt is required");
    }

    match name {
        "modelrouter_route" => match route(config, prompt, &arguments) {
            Ok(text) => tool_text_response(id, text),
            Err(message) => error_response(id, -32000, &message),
        },
        "modelrouter_run" => match run(config, prompt, &arguments, runner) {
            Ok(text) => tool_text_response(id, text),
            Err(message) => error_response(id, -32000, &message),
        },
        "modelrouter_broker" => match broker(config, prompt, &arguments, runner) {
            Ok(text) => tool_text_response(id, text),
            Err(message) => error_response(id, -32000, &message),
        },
        _ => error_response(id, -32602, "unknown tool"),
    }
}

fn route(config: &RouterConfig, prompt: &str, arguments: &Value) -> Result<String, String> {
    let request = route_request(prompt, arguments)?;
    let decision = Router::new(config.clone())
        .route(request)
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&decision).map_err(|error| error.to_string())
}

fn run<F>(
    config: &RouterConfig,
    prompt: &str,
    arguments: &Value,
    runner: F,
) -> Result<String, String>
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let request = route_request(prompt, arguments)?;
    let decision = Router::new(config.clone())
        .route(request)
        .map_err(|error| error.to_string())?;
    let provider = config
        .provider(decision.provider)
        .ok_or_else(|| format!("{} is not configured", decision.provider))?;
    let cwd = arguments.get("cwd").and_then(Value::as_str).map(Path::new);
    runner(provider, prompt, cwd)
}

fn broker<F>(
    config: &RouterConfig,
    prompt: &str,
    arguments: &Value,
    runner: F,
) -> Result<String, String>
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    let request = route_request(prompt, arguments)?;
    let decision = Router::new(config.clone())
        .route(request)
        .map_err(|error| error.to_string())?;
    let provider = config
        .provider(decision.provider)
        .ok_or_else(|| format!("{} is not configured", decision.provider))?;
    let cwd = arguments.get("cwd").and_then(Value::as_str).map(Path::new);
    let output = runner(provider, prompt, cwd)?;
    serde_json::to_string(&json!({
        "decision": decision,
        "provider": decision.provider,
        "output": output
    }))
    .map_err(|error| error.to_string())
}

fn route_request(prompt: &str, arguments: &Value) -> Result<RouteRequest, String> {
    let mut request = RouteRequest::new(prompt.to_string());
    if let Some(prefer) = arguments.get("prefer").and_then(Value::as_str) {
        request = request.prefer(prefer.parse()?);
    }
    Ok(request)
}

fn tool_text_response(id: Value, text: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                { "type": "text", "text": text }
            ]
        }
    })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn route_tool() -> Value {
    json!({
        "name": "modelrouter_route",
        "description": "Return a smart routing decision without running a model.",
        "inputSchema": tool_schema()
    })
}

fn run_tool() -> Value {
    json!({
        "name": "modelrouter_run",
        "description": "Route a prompt and run the selected local, subscription CLI, or HTTP-compatible provider.",
        "inputSchema": tool_schema()
    })
}

fn broker_tool() -> Value {
    json!({
        "name": "modelrouter_broker",
        "description": "Route a prompt, run the selected provider, and return both the route decision and output.",
        "inputSchema": tool_schema()
    })
}

fn tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string" },
            "prefer": {
                "type": "string",
                "enum": ["local", "codex", "claude", "gemini", "lmstudio", "llamacpp", "openai_compatible", "aider"]
            },
            "cwd": { "type": "string" }
        },
        "required": ["prompt"]
    })
}
