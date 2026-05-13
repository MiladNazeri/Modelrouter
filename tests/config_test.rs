use std::path::Path;

use modelrouter::{BillingMode, ProviderId, RouterConfig, load_config};

#[test]
fn default_config_includes_extra_provider_presets() {
    let config = RouterConfig::default();

    assert!(config.provider(ProviderId::Gemini).is_some());
    assert!(config.provider(ProviderId::LmStudio).is_some());
    assert!(config.provider(ProviderId::LlamaCpp).is_some());
    assert!(config.provider(ProviderId::OpenAiCompatible).is_some());
    assert!(config.provider(ProviderId::Aider).is_some());

    assert_eq!(
        config.provider(ProviderId::Gemini).expect("gemini").billing,
        BillingMode::Subscription
    );
    assert!(
        !config
            .provider(ProviderId::LmStudio)
            .expect("lm studio")
            .enabled
    );
}

#[test]
fn loads_toml_config_with_custom_local_model() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    std::fs::write(
        &config_path,
        r#"
[[providers]]
id = "local"
model = "qwen2.5-coder:7b"
enabled = true
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0
max_input_tokens = 32000
capabilities = ["local", "privacy", "summarization", "code"]

[[providers]]
id = "claude"
model = "claude-sonnet"
enabled = true
input_cost_per_million_tokens = 3.0
output_cost_per_million_tokens = 15.0
max_input_tokens = 200000
capabilities = ["reasoning", "writing", "long_context"]

[[providers]]
id = "codex"
model = "codex"
enabled = true
input_cost_per_million_tokens = 1.0
output_cost_per_million_tokens = 5.0
max_input_tokens = 128000
capabilities = ["code", "codebase_editing", "tests"]

[routing]
default_provider = "local"
code_provider = "codex"
reasoning_provider = "claude"
local_provider = "local"
"#,
    )
    .expect("write config");

    let config = load_config(&config_path).expect("config should parse");

    let local = config
        .provider(ProviderId::Local)
        .expect("local provider should exist");
    assert_eq!(local.model, "qwen2.5-coder:7b");
    assert_eq!(local.billing, BillingMode::Api);
    assert_eq!(config.routing.code_provider, ProviderId::Codex);
}

#[test]
fn rejects_routing_defaults_that_reference_missing_provider() {
    let mut config = RouterConfig::default();
    config
        .providers
        .retain(|provider| provider.id != ProviderId::Claude);
    config.routing.default_provider = ProviderId::Local;
    config.routing.reasoning_provider = ProviderId::Claude;

    let error = config
        .validate()
        .expect_err("config should reject missing provider default");

    assert!(
        error
            .to_string()
            .contains("reasoning_provider references missing provider")
    );
}

#[test]
fn example_config_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("modelrouter.example.toml");

    let config = load_config(&path).expect("example config should load");

    assert!(config.provider(ProviderId::Codex).expect("codex").enabled);
    assert!(
        !config
            .provider(ProviderId::OpenAiCompatible)
            .expect("openai-compatible")
            .enabled
    );
}
