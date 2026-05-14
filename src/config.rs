use std::{fmt, path::Path, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::router::RouterError;
use crate::spend::BudgetConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Local,
    Claude,
    Codex,
    Gemini,
    #[serde(alias = "lmstudio", alias = "lm-studio")]
    LmStudio,
    #[serde(alias = "llamacpp", alias = "llama.cpp", alias = "llama-cpp")]
    LlamaCpp,
    #[serde(
        alias = "openai_compatible",
        alias = "openai-compatible",
        alias = "openai"
    )]
    OpenAiCompatible,
    Aider,
}

impl ProviderId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::LmStudio => "LM Studio",
            Self::LlamaCpp => "llama.cpp",
            Self::OpenAiCompatible => "OpenAI-compatible",
            Self::Aider => "Aider",
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Local => "local",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::LmStudio => "lmstudio",
            Self::LlamaCpp => "llamacpp",
            Self::OpenAiCompatible => "openai_compatible",
            Self::Aider => "aider",
        };
        formatter.write_str(value)
    }
}

impl FromStr for ProviderId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "lmstudio" | "lm-studio" | "lm_studio" => Ok(Self::LmStudio),
            "llamacpp" | "llama.cpp" | "llama-cpp" | "llama_cpp" => Ok(Self::LlamaCpp),
            "openai_compatible" | "openai-compatible" | "openai" => Ok(Self::OpenAiCompatible),
            "aider" => Ok(Self::Aider),
            other => Err(format!(
                "invalid provider: {other}. expected local, claude, codex, gemini, lmstudio, llamacpp, openai_compatible, or aider"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Local,
    Privacy,
    Summarization,
    Code,
    CodebaseEditing,
    Tests,
    Reasoning,
    Writing,
    LongContext,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingMode {
    #[default]
    Api,
    Subscription,
    Local,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    OpenAiCompatible,
    SubscriptionCli,
    LocalCli,
    AgentCli,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskHint {
    Simple,
    Code,
    DeepReasoning,
    Writing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub model: String,
    pub enabled: bool,
    #[serde(default)]
    pub kind: ProviderKind,
    #[serde(default)]
    pub billing: BillingMode,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
    pub max_input_tokens: u32,
    pub capabilities: Vec<Capability>,
}

impl ProviderConfig {
    pub fn estimated_cost_cents(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        if self.billing != BillingMode::Api {
            return 0.0;
        }

        let input_cost =
            (f64::from(input_tokens) / 1_000_000.0) * self.input_cost_per_million_tokens;
        let output_cost =
            (f64::from(output_tokens) / 1_000_000.0) * self.output_cost_per_million_tokens;
        input_cost + output_cost
    }

    pub fn supports(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub default_provider: ProviderId,
    pub code_provider: ProviderId,
    pub reasoning_provider: ProviderId,
    pub local_provider: ProviderId,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            default_provider: ProviderId::Claude,
            code_provider: ProviderId::Codex,
            reasoning_provider: ProviderId::Claude,
            local_provider: ProviderId::Local,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouterConfig {
    pub providers: Vec<ProviderConfig>,
    pub routing: RoutingConfig,
    #[serde(default)]
    pub rules: Vec<RouteRule>,
    #[serde(default)]
    pub profiles: Vec<ProjectProfile>,
    #[serde(default)]
    pub favorites: Vec<PathFavorite>,
    #[serde(default)]
    pub budget: BudgetConfig,
    #[serde(default)]
    pub server: ServerConfig,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub auth_token: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteMatch {
    #[serde(default)]
    pub task: Option<TaskHint>,
    #[serde(default)]
    pub private: Option<bool>,
    #[serde(default)]
    pub long_context: Option<bool>,
    #[serde(default)]
    pub repo: Option<bool>,
    #[serde(default)]
    pub max_input_tokens: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteRule {
    pub name: String,
    pub when: RouteMatch,
    pub prefer: ProviderId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub name: String,
    pub path_contains: String,
    #[serde(default)]
    pub default_provider: Option<ProviderId>,
    #[serde(default)]
    pub code_provider: Option<ProviderId>,
    #[serde(default)]
    pub reasoning_provider: Option<ProviderId>,
    #[serde(default)]
    pub local_provider: Option<ProviderId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PathFavorite {
    pub name: String,
    pub path: String,
}

impl RouterConfig {
    pub fn provider(&self, id: ProviderId) -> Option<&ProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    pub fn disable_provider(&mut self, id: ProviderId) {
        if let Some(provider) = self.providers.iter_mut().find(|provider| provider.id == id) {
            provider.enabled = false;
        }
    }

    pub fn validate(&self) -> Result<(), RouterError> {
        self.ensure_provider_exists("default_provider", self.routing.default_provider)?;
        self.ensure_provider_exists("code_provider", self.routing.code_provider)?;
        self.ensure_provider_exists("reasoning_provider", self.routing.reasoning_provider)?;
        self.ensure_provider_exists("local_provider", self.routing.local_provider)?;
        for rule in &self.rules {
            self.ensure_provider_exists("route rule prefer", rule.prefer)?;
        }
        for profile in &self.profiles {
            if let Some(provider) = profile.default_provider {
                self.ensure_provider_exists("profile default_provider", provider)?;
            }
            if let Some(provider) = profile.code_provider {
                self.ensure_provider_exists("profile code_provider", provider)?;
            }
            if let Some(provider) = profile.reasoning_provider {
                self.ensure_provider_exists("profile reasoning_provider", provider)?;
            }
            if let Some(provider) = profile.local_provider {
                self.ensure_provider_exists("profile local_provider", provider)?;
            }
        }
        Ok(())
    }

    fn ensure_provider_exists(&self, field: &str, id: ProviderId) -> Result<(), RouterError> {
        if self.provider(id).is_none() {
            return Err(RouterError::InvalidConfig(format!(
                "{field} references missing provider {id}"
            )));
        }
        Ok(())
    }
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            providers: vec![
                ProviderConfig {
                    id: ProviderId::Local,
                    model: "llama3.2:3b".to_string(),
                    enabled: true,
                    kind: ProviderKind::LocalCli,
                    billing: BillingMode::Local,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 0.0,
                    output_cost_per_million_tokens: 0.0,
                    max_input_tokens: 32_000,
                    capabilities: vec![
                        Capability::Local,
                        Capability::Privacy,
                        Capability::Summarization,
                        Capability::Code,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Claude,
                    model: "sonnet".to_string(),
                    enabled: true,
                    kind: ProviderKind::SubscriptionCli,
                    billing: BillingMode::Subscription,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 3.0,
                    output_cost_per_million_tokens: 15.0,
                    max_input_tokens: 200_000,
                    capabilities: vec![
                        Capability::Reasoning,
                        Capability::Writing,
                        Capability::LongContext,
                        Capability::Summarization,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Codex,
                    model: "auto".to_string(),
                    enabled: true,
                    kind: ProviderKind::SubscriptionCli,
                    billing: BillingMode::Subscription,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 1.0,
                    output_cost_per_million_tokens: 5.0,
                    max_input_tokens: 128_000,
                    capabilities: vec![
                        Capability::Code,
                        Capability::CodebaseEditing,
                        Capability::Tests,
                        Capability::Reasoning,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Gemini,
                    model: "gemini-2.5-pro".to_string(),
                    enabled: true,
                    kind: ProviderKind::SubscriptionCli,
                    billing: BillingMode::Subscription,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 0.0,
                    output_cost_per_million_tokens: 0.0,
                    max_input_tokens: 1_000_000,
                    capabilities: vec![
                        Capability::Code,
                        Capability::Reasoning,
                        Capability::Writing,
                        Capability::LongContext,
                        Capability::Summarization,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::LmStudio,
                    model: "local-model".to_string(),
                    enabled: false,
                    kind: ProviderKind::OpenAiCompatible,
                    billing: BillingMode::Local,
                    endpoint_url: Some("http://localhost:1234/v1".to_string()),
                    input_cost_per_million_tokens: 0.0,
                    output_cost_per_million_tokens: 0.0,
                    max_input_tokens: 128_000,
                    capabilities: vec![
                        Capability::Local,
                        Capability::Privacy,
                        Capability::Summarization,
                        Capability::Code,
                        Capability::Reasoning,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::LlamaCpp,
                    model: "local-model".to_string(),
                    enabled: false,
                    kind: ProviderKind::OpenAiCompatible,
                    billing: BillingMode::Local,
                    endpoint_url: Some("http://localhost:8080/v1".to_string()),
                    input_cost_per_million_tokens: 0.0,
                    output_cost_per_million_tokens: 0.0,
                    max_input_tokens: 128_000,
                    capabilities: vec![
                        Capability::Local,
                        Capability::Privacy,
                        Capability::Summarization,
                        Capability::Code,
                        Capability::Reasoning,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::OpenAiCompatible,
                    model: "model".to_string(),
                    enabled: false,
                    kind: ProviderKind::OpenAiCompatible,
                    billing: BillingMode::Api,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 1.0,
                    output_cost_per_million_tokens: 5.0,
                    max_input_tokens: 128_000,
                    capabilities: vec![
                        Capability::Summarization,
                        Capability::Code,
                        Capability::Reasoning,
                        Capability::Writing,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Aider,
                    model: "auto".to_string(),
                    enabled: false,
                    kind: ProviderKind::AgentCli,
                    billing: BillingMode::Api,
                    endpoint_url: None,
                    input_cost_per_million_tokens: 1.0,
                    output_cost_per_million_tokens: 5.0,
                    max_input_tokens: 128_000,
                    capabilities: vec![
                        Capability::Code,
                        Capability::CodebaseEditing,
                        Capability::Tests,
                    ],
                },
            ],
            routing: RoutingConfig::default(),
            rules: Vec::new(),
            profiles: Vec::new(),
            favorites: Vec::new(),
            budget: BudgetConfig::default(),
            server: ServerConfig::default(),
        }
    }
}

pub fn load_config(path: &Path) -> Result<RouterConfig, RouterError> {
    let contents = std::fs::read_to_string(path).map_err(|source| RouterError::ConfigIo {
        path: path.to_path_buf(),
        source,
    })?;
    let config = toml::from_str::<RouterConfig>(&contents)
        .map_err(|source| RouterError::ConfigParse { source })?;
    config.validate()?;
    Ok(config)
}

pub fn apply_project_profile(config: &RouterConfig, cwd: &Path) -> RouterConfig {
    let mut profiled = config.clone();
    let cwd = cwd.display().to_string();
    let Some(profile) = config
        .profiles
        .iter()
        .find(|profile| cwd.contains(&profile.path_contains))
    else {
        return profiled;
    };

    if let Some(provider) = profile.default_provider {
        profiled.routing.default_provider = provider;
    }
    if let Some(provider) = profile.code_provider {
        profiled.routing.code_provider = provider;
    }
    if let Some(provider) = profile.reasoning_provider {
        profiled.routing.reasoning_provider = provider;
    }
    if let Some(provider) = profile.local_provider {
        profiled.routing.local_provider = provider;
    }
    profiled
}
