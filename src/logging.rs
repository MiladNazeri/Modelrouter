use std::{
    fmt::Write as FmtWrite,
    fs::OpenOptions,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{RouteDecision, TokenUsage};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequestLogEntry {
    pub timestamp_unix_ms: u128,
    pub operation: String,
    pub prompt_hash: String,
    pub provider: String,
    pub model: String,
    pub billing: String,
    pub estimated_input_tokens: u32,
    pub estimated_output_tokens: u32,
    pub estimated_cost_cents: f64,
    #[serde(default)]
    pub actual_input_tokens: Option<u32>,
    #[serde(default)]
    pub actual_output_tokens: Option<u32>,
    #[serde(default)]
    pub actual_cost_cents: Option<f64>,
    #[serde(default)]
    pub spend_source: Option<String>,
    pub latency_ms: u128,
    pub success: bool,
    pub error: Option<String>,
    pub reasons: Vec<String>,
}

impl RequestLogEntry {
    pub fn from_decision(
        operation: &str,
        prompt: &str,
        decision: &RouteDecision,
        latency_ms: u128,
        success: bool,
        error: Option<String>,
    ) -> Self {
        Self {
            timestamp_unix_ms: now_unix_ms(),
            operation: operation.to_string(),
            prompt_hash: prompt_hash(prompt),
            provider: decision.provider.to_string(),
            model: decision.model.clone(),
            billing: serde_json::to_value(decision.billing)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            estimated_input_tokens: decision.estimated_input_tokens,
            estimated_output_tokens: decision.estimated_output_tokens,
            estimated_cost_cents: decision.estimated_cost_cents,
            actual_input_tokens: None,
            actual_output_tokens: None,
            actual_cost_cents: None,
            spend_source: None,
            latency_ms,
            success,
            error,
            reasons: decision.reasons.clone(),
        }
    }

    pub fn from_decision_with_actual_usage(
        operation: &str,
        prompt: &str,
        decision: &RouteDecision,
        usage: TokenUsage,
        actual_cost_cents: Option<f64>,
        latency_ms: u128,
    ) -> Self {
        let mut entry = Self::from_decision(operation, prompt, decision, latency_ms, true, None);
        entry.actual_input_tokens = Some(usage.input_tokens);
        entry.actual_output_tokens = Some(usage.output_tokens);
        entry.actual_cost_cents = actual_cost_cents;
        entry.spend_source = Some("provider_response".to_string());
        entry
    }
}

pub fn append_request_log(path: &Path, entry: &RequestLogEntry) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn prompt_hash(prompt: &str) -> String {
    let digest = Sha256::digest(prompt.as_bytes());
    let mut hash = String::with_capacity(64);
    for byte in digest {
        FmtWrite::write_fmt(&mut hash, format_args!("{byte:02x}"))
            .expect("writing to a String cannot fail");
    }
    hash
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
