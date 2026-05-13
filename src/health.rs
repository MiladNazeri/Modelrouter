use std::{env, path::Path, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{ProviderConfig, ProviderId, ProviderKind, RouterConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Available,
    Unavailable,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider: ProviderId,
    pub enabled: bool,
    pub status: ProviderHealthStatus,
    pub check: String,
    pub message: String,
}

pub fn health_report(config: &RouterConfig) -> Vec<ProviderHealth> {
    health_report_with(config, command_exists, http_endpoint_available)
}

pub fn health_report_with<C, H>(
    config: &RouterConfig,
    command_exists: C,
    http_endpoint_available: H,
) -> Vec<ProviderHealth>
where
    C: Fn(&str) -> bool,
    H: Fn(&str) -> bool,
{
    config
        .providers
        .iter()
        .map(|provider| provider_health(provider, &command_exists, &http_endpoint_available))
        .collect()
}

pub fn availability_filtered_config(
    config: &RouterConfig,
    report: &[ProviderHealth],
) -> RouterConfig {
    let mut filtered = config.clone();
    for health in report {
        if health.enabled
            && health.status != ProviderHealthStatus::Available
            && let Some(provider) = filtered
                .providers
                .iter_mut()
                .find(|provider| provider.id == health.provider)
        {
            provider.enabled = false;
        }
    }
    filtered
}

fn provider_health<C, H>(
    provider: &ProviderConfig,
    command_exists: &C,
    http_endpoint_available: &H,
) -> ProviderHealth
where
    C: Fn(&str) -> bool,
    H: Fn(&str) -> bool,
{
    if !provider.enabled {
        return ProviderHealth {
            provider: provider.id,
            enabled: false,
            status: ProviderHealthStatus::Disabled,
            check: "disabled".to_string(),
            message: "Provider is disabled in config.".to_string(),
        };
    }

    match provider.kind {
        ProviderKind::SubscriptionCli | ProviderKind::LocalCli | ProviderKind::AgentCli => {
            let Some(program) = provider_program(provider.id) else {
                return unavailable(provider, "command", "No CLI program is configured.");
            };
            if command_exists(program) {
                ProviderHealth {
                    provider: provider.id,
                    enabled: true,
                    status: ProviderHealthStatus::Available,
                    check: format!("command:{program}"),
                    message: format!("{program} is available on PATH."),
                }
            } else {
                unavailable(
                    provider,
                    &format!("command:{program}"),
                    &format!("{program} is not available on PATH."),
                )
            }
        }
        ProviderKind::OpenAiCompatible => {
            let Some(endpoint) = provider.endpoint_url.as_deref() else {
                return unavailable(provider, "http", "No endpoint_url is configured.");
            };
            if http_endpoint_available(endpoint) {
                ProviderHealth {
                    provider: provider.id,
                    enabled: true,
                    status: ProviderHealthStatus::Available,
                    check: format!("http:{endpoint}"),
                    message: "Endpoint is reachable.".to_string(),
                }
            } else {
                unavailable(
                    provider,
                    &format!("http:{endpoint}"),
                    "Endpoint is not reachable.",
                )
            }
        }
    }
}

fn unavailable(provider: &ProviderConfig, check: &str, message: &str) -> ProviderHealth {
    ProviderHealth {
        provider: provider.id,
        enabled: provider.enabled,
        status: ProviderHealthStatus::Unavailable,
        check: check.to_string(),
        message: message.to_string(),
    }
}

fn provider_program(provider: ProviderId) -> Option<&'static str> {
    match provider {
        ProviderId::Codex => Some("codex"),
        ProviderId::Claude => Some("claude"),
        ProviderId::Gemini => Some("gemini"),
        ProviderId::Local => Some("ollama"),
        ProviderId::Aider => Some("aider"),
        ProviderId::LmStudio | ProviderId::LlamaCpp | ProviderId::OpenAiCompatible => None,
    }
}

fn command_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|path| path.join(program).is_file())
}

fn http_endpoint_available(endpoint: &str) -> bool {
    let url = endpoint.trim_end_matches('/');
    let url = if url.ends_with("/models") {
        url.to_string()
    } else {
        format!("{url}/models")
    };
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(700))
        .build()
        .and_then(|client| client.get(url).send())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}
