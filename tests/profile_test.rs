use std::path::Path;

use modelrouter::{ProviderId, RouterConfig, apply_project_profile};

#[test]
fn project_profile_can_override_routing_for_matching_paths() {
    let config = r#"
[[providers]]
id = "local"
model = "llama3.2:3b"
enabled = true
kind = "local_cli"
billing = "local"
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0
max_input_tokens = 32000
capabilities = ["local", "privacy", "summarization", "code"]

[[providers]]
id = "claude"
model = "sonnet"
enabled = true
kind = "subscription_cli"
billing = "subscription"
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0
max_input_tokens = 200000
capabilities = ["reasoning", "writing", "long_context", "summarization"]

[[providers]]
id = "codex"
model = "auto"
enabled = true
kind = "subscription_cli"
billing = "subscription"
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0
max_input_tokens = 128000
capabilities = ["code", "codebase_editing", "tests", "reasoning"]

[routing]
default_provider = "local"
code_provider = "codex"
reasoning_provider = "claude"
local_provider = "local"

[[profiles]]
name = "router-repo"
path_contains = "Modelrouter"
code_provider = "claude"
default_provider = "claude"
"#;
    let config: RouterConfig = toml::from_str(config).expect("config");

    let profiled = apply_project_profile(
        &config,
        Path::new("/Users/m/clor/c/code/_Working/Modelrouter"),
    );

    assert_eq!(profiled.routing.code_provider, ProviderId::Claude);
    assert_eq!(profiled.routing.default_provider, ProviderId::Claude);
}
