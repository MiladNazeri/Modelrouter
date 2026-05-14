use std::path::Path;

use serde_json::Value;
use thiserror::Error;

use crate::{ClassificationMode, ProviderConfig, RouteClassification, RouterConfig, TaskHint};

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ClassificationError {
    #[error("classifier response did not contain a JSON object")]
    MissingJson,
    #[error("classifier response JSON was invalid: {0}")]
    InvalidJson(String),
    #[error("classifier response task was invalid: {0}")]
    InvalidTask(String),
    #[error("classifier response did not contain any routing signals")]
    Empty,
}

pub fn classify_prompt_with_runner<F>(
    config: &RouterConfig,
    prompt: &str,
    runner: F,
) -> Result<Option<RouteClassification>, String>
where
    F: Fn(&ProviderConfig, &str, Option<&Path>) -> Result<String, String>,
{
    if config.classification.mode != ClassificationMode::Llm {
        return Ok(None);
    }
    let provider_id = config
        .classification
        .provider
        .unwrap_or(config.routing.local_provider);
    let Some(provider) = config.provider(provider_id) else {
        return Err(format!(
            "classifier provider {provider_id} is not configured"
        ));
    };
    if !provider.enabled {
        return Err(format!("classifier provider {provider_id} is disabled"));
    }
    let classifier_prompt =
        build_classification_prompt(prompt, config.classification.max_prompt_chars);
    let output = runner(provider, &classifier_prompt, None)?;
    parse_classification_response(&output)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn build_classification_prompt(prompt: &str, max_prompt_chars: usize) -> String {
    let prompt = truncate_chars(prompt, max_prompt_chars);
    format!(
        "Classify this request for an AI model router. Return JSON only, with no markdown. \
Schema: {{\"task\":\"simple|code|deep_reasoning|writing\",\"private\":true|false,\"long_context\":true|false,\"repo\":true|false,\"confidence\":0.0,\"reason\":\"brief\"}}. \
Do not choose a provider. Only classify the request.\n\nREQUEST:\n{prompt}"
    )
}

pub fn parse_classification_response(
    output: &str,
) -> Result<RouteClassification, ClassificationError> {
    let json_text = extract_json_object(output)?;
    let value = serde_json::from_str::<Value>(json_text)
        .map_err(|error| ClassificationError::InvalidJson(error.to_string()))?;
    let task = match value.get("task").and_then(Value::as_str) {
        Some(task) if !task.trim().is_empty() => Some(parse_task_hint(task)?),
        _ => None,
    };
    let private = value.get("private").and_then(Value::as_bool);
    let long_context = value.get("long_context").and_then(Value::as_bool);
    let repo = value.get("repo").and_then(Value::as_bool);
    let confidence = value
        .get("confidence")
        .and_then(Value::as_f64)
        .map(|confidence| confidence.clamp(0.0, 1.0) as f32);
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(|reason| reason.trim().chars().take(240).collect::<String>())
        .filter(|reason| !reason.is_empty());
    if task.is_none()
        && private.is_none()
        && long_context.is_none()
        && repo.is_none()
        && confidence.is_none()
        && reason.is_none()
    {
        return Err(ClassificationError::Empty);
    }
    Ok(RouteClassification {
        task,
        private,
        long_context,
        repo,
        confidence,
        reason,
    })
}

fn parse_task_hint(value: &str) -> Result<TaskHint, ClassificationError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "simple" => Ok(TaskHint::Simple),
        "code" => Ok(TaskHint::Code),
        "deep_reasoning" | "deep-reasoning" | "reasoning" => Ok(TaskHint::DeepReasoning),
        "writing" | "write" => Ok(TaskHint::Writing),
        other => Err(ClassificationError::InvalidTask(other.to_string())),
    }
}

fn extract_json_object(output: &str) -> Result<&str, ClassificationError> {
    let start = output.find('{').ok_or(ClassificationError::MissingJson)?;
    let end = output.rfind('}').ok_or(ClassificationError::MissingJson)?;
    if end < start {
        return Err(ClassificationError::MissingJson);
    }
    Ok(&output[start..=end])
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        truncated.push_str("\n[truncated]");
    }
    truncated
}
