use serde_json::{Value, json};
use thiserror::Error;

use crate::ProviderConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct HttpProviderRequest {
    pub url: String,
    pub body: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpProviderOutput {
    pub content: String,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Error)]
pub enum ProviderHttpError {
    #[error("provider {provider} does not have an endpoint_url")]
    MissingEndpoint { provider: String },
    #[error("provider endpoint must start with http:// or https://: {url}")]
    InvalidEndpoint { url: String },
    #[error("http request to {url} failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("provider returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("provider response was not valid json: {source}")]
    InvalidJson { source: serde_json::Error },
    #[error("provider response did not include assistant content")]
    MissingContent,
}

pub fn build_http_provider_request(
    provider: &ProviderConfig,
    prompt: &str,
) -> Result<HttpProviderRequest, ProviderHttpError> {
    let endpoint =
        provider
            .endpoint_url
            .as_deref()
            .ok_or_else(|| ProviderHttpError::MissingEndpoint {
                provider: provider.id.to_string(),
            })?;

    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return Err(ProviderHttpError::InvalidEndpoint {
            url: endpoint.to_string(),
        });
    }

    let base = endpoint.trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    };

    Ok(HttpProviderRequest {
        url,
        body: json!({
            "model": provider.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt,
                }
            ],
            "stream": false,
        }),
    })
}

pub fn run_http_provider(
    provider: &ProviderConfig,
    prompt: &str,
) -> Result<String, ProviderHttpError> {
    Ok(run_http_provider_with_usage(provider, prompt)?.content)
}

pub fn run_http_provider_with_usage(
    provider: &ProviderConfig,
    prompt: &str,
) -> Result<HttpProviderOutput, ProviderHttpError> {
    let request = build_http_provider_request(provider, prompt)?;
    let response = reqwest::blocking::Client::new()
        .post(&request.url)
        .json(&request.body)
        .send()
        .map_err(|source| ProviderHttpError::Request {
            url: request.url.clone(),
            source,
        })?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|source| ProviderHttpError::Request {
            url: request.url.clone(),
            source,
        })?;

    if !status.is_success() {
        return Err(ProviderHttpError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }

    parse_openai_compatible_response_with_usage(&body)
}

pub fn parse_openai_compatible_response(body: &str) -> Result<String, ProviderHttpError> {
    Ok(parse_openai_compatible_response_with_usage(body)?.content)
}

pub fn parse_openai_compatible_response_with_usage(
    body: &str,
) -> Result<HttpProviderOutput, ProviderHttpError> {
    let value = serde_json::from_str::<Value>(body)
        .map_err(|source| ProviderHttpError::InvalidJson { source })?;

    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
        .or_else(|| value.get("output_text").and_then(Value::as_str))
        .ok_or(ProviderHttpError::MissingContent)?;

    Ok(HttpProviderOutput {
        content: content.trim().to_string(),
        usage: parse_usage(&value),
    })
}

fn parse_usage(value: &Value) -> Option<TokenUsage> {
    let usage = value.get("usage")?;
    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())?;
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())?;
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));
    Some(TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}
