use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::Instant,
};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    BillingMode, Capability, ClassificationConfig, PathFavorite, ProjectProfile, ProviderConfig,
    ProviderHealth, ProviderId, ProviderKind, RequestLogEntry, RouteDecision, RouteMatch,
    RouteRequest, RouteRule, Router, RouterConfig, RoutingConfig, ServerConfig, SpendWindow,
    TaskHint, append_request_log, apply_project_profile, build_anthropic_cost_request,
    build_openai_cost_request, classify_prompt_with_runner, health_report, load_config,
    load_metrics, parse_anthropic_cost_report, parse_openai_costs_response, run_provider,
    run_provider_with_usage, run_tui, serve_mcp, serve_with_log_and_config_path,
    sync_anthropic_cost_report, sync_openai_costs, try_build_google_billing_query,
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Route AI tasks to local, Codex, or Claude models by cost and fit."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Tui {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    Chat {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    Init {
        #[arg(long, default_value = "modelrouter.toml")]
        config: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        local_endpoint: Option<String>,
        #[arg(long)]
        auth_token: Option<String>,
    },
    Doctor {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Route {
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        prefer: Option<ProviderId>,
        #[arg(long)]
        max_cost_cents: Option<f64>,
        #[arg(long)]
        input_tokens: Option<u32>,
        #[arg(long)]
        output_tokens: Option<u32>,
        #[arg(long)]
        hint: Option<CliTaskHint>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    Run {
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        prefer: Option<ProviderId>,
        #[arg(long)]
        max_cost_cents: Option<f64>,
        #[arg(long)]
        input_tokens: Option<u32>,
        #[arg(long)]
        output_tokens: Option<u32>,
        #[arg(long)]
        hint: Option<CliTaskHint>,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long)]
        log: Option<PathBuf>,
    },
    Serve {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long)]
        log: Option<PathBuf>,
    },
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    Mcp {
        #[command(subcommand)]
        command: Option<McpCommand>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Health {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Spend {
        #[command(subcommand)]
        command: SpendCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    Start {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long)]
        open: bool,
    },
    LaunchdPlist {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long, default_value = "com.modelrouter.daemon")]
        label: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    InstallConfig {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        command: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum SpendCommand {
    Report {
        #[arg(long)]
        log: PathBuf,
        #[arg(long)]
        json: bool,
    },
    OpenaiRequest {
        #[arg(long)]
        start_time: u64,
        #[arg(long)]
        end_time: u64,
        #[arg(long, default_value = "OPENAI_ADMIN_KEY")]
        api_key_env: String,
    },
    AnthropicRequest {
        #[arg(long)]
        starting_at: String,
        #[arg(long)]
        ending_at: String,
        #[arg(long, default_value = "ANTHROPIC_ADMIN_KEY")]
        api_key_env: String,
    },
    GoogleQuery {
        #[arg(long)]
        table: String,
        #[arg(long)]
        start_date: String,
        #[arg(long)]
        end_date: String,
    },
    ParseOpenai {
        #[arg(long)]
        file: PathBuf,
    },
    ParseAnthropic {
        #[arg(long)]
        file: PathBuf,
    },
    SyncOpenai {
        #[arg(long)]
        start_time: u64,
        #[arg(long)]
        end_time: u64,
        #[arg(long, default_value = "OPENAI_ADMIN_KEY")]
        api_key_env: String,
    },
    SyncAnthropic {
        #[arg(long)]
        starting_at: String,
        #[arg(long)]
        ending_at: String,
        #[arg(long, default_value = "ANTHROPIC_ADMIN_KEY")]
        api_key_env: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliTaskHint {
    Simple,
    Code,
    DeepReasoning,
    Writing,
}

impl From<CliTaskHint> for TaskHint {
    fn from(value: CliTaskHint) -> Self {
        match value {
            CliTaskHint::Simple => Self::Simple,
            CliTaskHint::Code => Self::Code,
            CliTaskHint::DeepReasoning => Self::DeepReasoning,
            CliTaskHint::Writing => Self::Writing,
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            let config = load_config_or_default(None)?;
            run_tui(config, Some(default_log_path()), None)?;
        }
        Some(Command::Tui { config, log, cwd }) | Some(Command::Chat { config, log, cwd }) => {
            let config = load_config_or_default(config)?;
            run_tui(config, Some(log.unwrap_or_else(default_log_path)), cwd)?;
        }
        Some(Command::Init {
            config,
            force,
            local_endpoint,
            auth_token,
        }) => init_config(
            &config,
            force,
            local_endpoint.as_deref(),
            auth_token.as_deref(),
        )?,
        Some(Command::Doctor { config, json }) => run_doctor(config, json)?,
        Some(Command::Route {
            prompt,
            config,
            prefer,
            max_cost_cents,
            input_tokens,
            output_tokens,
            hint,
            json,
            log,
            cwd,
        }) => {
            let config = load_config_or_default(config)?;
            let config = config_for_cwd(config, cwd.as_deref())?;
            let mut request = RouteRequest::new(prompt.clone());

            if let Some(provider) = prefer {
                request = request.prefer(provider);
            }
            if let Some(max_cost_cents) = max_cost_cents {
                request = request.with_max_cost_cents(max_cost_cents);
            }
            if let Some(tokens) = input_tokens {
                request = request.with_estimated_input_tokens(tokens);
            }
            if let Some(tokens) = output_tokens {
                request = request.with_estimated_output_tokens(tokens);
            }
            if let Some(hint) = hint {
                request = request.with_hint(hint.into());
            }

            let start = Instant::now();
            let (request, fallback_reasons) = maybe_classify_cli_request(
                &config,
                request,
                &prompt,
                prefer.is_some(),
                hint.is_some(),
            );
            let mut decision = Router::new(config).route(request)?;
            prepend_reasons(&mut decision, fallback_reasons);
            append_log(
                log.as_deref(),
                "route",
                &prompt,
                &decision,
                start,
                true,
                None,
            )?;
            if json {
                println!("{}", serde_json::to_string(&decision)?);
            } else {
                println!("provider: {}", decision.provider);
                println!("model: {}", decision.model);
                println!("estimated_cost_cents: {:.6}", decision.estimated_cost_cents);
                println!("confidence: {:.2}", decision.confidence);
                for reason in decision.reasons {
                    println!("reason: {reason}");
                }
            }
        }
        Some(Command::Run {
            prompt,
            config,
            prefer,
            max_cost_cents,
            input_tokens,
            output_tokens,
            hint,
            cwd,
            log,
        }) => {
            let config = load_config_or_default(config)?;
            let config = config_for_cwd(config, cwd.as_deref())?;
            let router = Router::new(config.clone());
            let mut request = RouteRequest::new(prompt.clone());

            if let Some(provider) = prefer {
                request = request.prefer(provider);
            }
            if let Some(max_cost_cents) = max_cost_cents {
                request = request.with_max_cost_cents(max_cost_cents);
            }
            if let Some(tokens) = input_tokens {
                request = request.with_estimated_input_tokens(tokens);
            }
            if let Some(tokens) = output_tokens {
                request = request.with_estimated_output_tokens(tokens);
            }
            if let Some(hint) = hint {
                request = request.with_hint(hint.into());
            }

            let start = Instant::now();
            let (request, fallback_reasons) = maybe_classify_cli_request(
                &config,
                request,
                &prompt,
                prefer.is_some(),
                hint.is_some(),
            );
            let mut decision = router.route(request)?;
            prepend_reasons(&mut decision, fallback_reasons);
            let provider = config.provider(decision.provider).ok_or(
                crate::ProviderRunError::MissingProvider {
                    provider: decision.provider,
                },
            )?;
            let output = run_provider_with_usage(provider, &prompt, cwd.as_deref())?;
            append_log_result(log.as_deref(), "run", &prompt, &decision, start, &output)?;
            println!("{}", output.output);
        }
        Some(Command::Serve {
            config,
            host,
            port,
            log,
        }) => {
            let config_path = resolved_config_path(config.clone());
            let config = load_config_or_default(config)?;
            let address = format!("{host}:{port}");
            eprintln!("modelrouter serving on http://{address}");
            serve_with_log_and_config_path(config, address, log, config_path)?;
        }
        Some(Command::Daemon { command }) => run_daemon(command)?,
        Some(Command::Mcp { command, config }) => {
            if let Some(command) = command {
                run_mcp_command(command, config)?;
            } else {
                let config = load_config_or_default(config)?;
                serve_mcp(config)?;
            }
        }
        Some(Command::Health { config, json }) => {
            let config = load_config_or_default(config)?;
            let report = health_report(&config);
            if json {
                println!("{}", serde_json::json!({ "providers": report }));
            } else {
                for provider in report {
                    println!(
                        "{}: {:?} ({}) {}",
                        provider.provider, provider.status, provider.check, provider.message
                    );
                }
            }
        }
        Some(Command::Spend { command }) => run_spend(command)?,
    }

    Ok(())
}

fn init_config(
    path: &Path,
    force: bool,
    local_endpoint: Option<&str>,
    auth_token: Option<&str>,
) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; rerun with --force to overwrite it",
            path.display()
        );
    }

    let config = detected_config(local_endpoint, auth_token);
    let mut contents = String::from(
        "# Local Modelrouter config. Keep this file private; it may contain local endpoints.\n",
    );
    contents.push_str(
        "# API providers are disabled by default; subscription and local routes cost $0 here.\n\n",
    );
    contents.push_str(&toml::to_string_pretty(&config)?);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    println!("wrote {}", path.display());
    println!("next: modelrouter doctor --config {}", path.display());
    Ok(())
}

fn detected_config(local_endpoint: Option<&str>, auth_token: Option<&str>) -> RouterConfig {
    let local_endpoint = local_endpoint
        .map(normalize_openai_base_endpoint)
        .or_else(|| {
            env::var("MODELROUTER_LOCAL_ENDPOINT")
                .ok()
                .map(|value| normalize_openai_base_endpoint(&value))
        })
        .or_else(|| {
            endpoint_available("http://localhost:11434/v1")
                .then(|| "http://localhost:11434/v1".to_string())
        });
    let local_model = local_endpoint
        .as_deref()
        .and_then(detect_openai_compatible_model)
        .unwrap_or_else(|| "llama3.2:latest".to_string());
    let local_enabled = local_endpoint.is_some() || command_available("ollama");
    let local_kind = if local_endpoint.is_some() {
        ProviderKind::OpenAiCompatible
    } else {
        ProviderKind::LocalCli
    };

    RouterConfig {
        providers: vec![
            ProviderConfig {
                id: ProviderId::Local,
                model: local_model,
                enabled: local_enabled,
                kind: local_kind,
                billing: BillingMode::Local,
                endpoint_url: local_endpoint,
                input_cost_per_million_tokens: 0.0,
                output_cost_per_million_tokens: 0.0,
                max_input_tokens: 128_000,
                capabilities: vec![
                    Capability::Local,
                    Capability::Privacy,
                    Capability::Summarization,
                    Capability::Code,
                    Capability::Reasoning,
                    Capability::Writing,
                ],
            },
            subscription_provider(
                ProviderId::Claude,
                "sonnet",
                "claude",
                200_000,
                vec![
                    Capability::Reasoning,
                    Capability::Writing,
                    Capability::LongContext,
                    Capability::Summarization,
                    Capability::Code,
                ],
            ),
            subscription_provider(
                ProviderId::Codex,
                "auto",
                "codex",
                128_000,
                vec![
                    Capability::Code,
                    Capability::CodebaseEditing,
                    Capability::Tests,
                    Capability::Reasoning,
                ],
            ),
            subscription_provider(
                ProviderId::Gemini,
                "gemini-2.5-pro",
                "gemini",
                1_000_000,
                vec![
                    Capability::Code,
                    Capability::Reasoning,
                    Capability::Writing,
                    Capability::LongContext,
                    Capability::Summarization,
                ],
            ),
            local_http_provider(
                ProviderId::LmStudio,
                "local-model",
                "http://localhost:1234/v1",
                endpoint_available("http://localhost:1234/v1"),
            ),
            local_http_provider(
                ProviderId::LlamaCpp,
                "local-model",
                "http://localhost:8080/v1",
                endpoint_available("http://localhost:8080/v1"),
            ),
            ProviderConfig {
                id: ProviderId::OpenAiCompatible,
                model: "gpt-5.4-mini".to_string(),
                enabled: false,
                kind: ProviderKind::OpenAiCompatible,
                billing: BillingMode::Api,
                endpoint_url: Some("https://api.openai.com/v1".to_string()),
                input_cost_per_million_tokens: 0.75,
                output_cost_per_million_tokens: 4.5,
                max_input_tokens: 128_000,
                capabilities: vec![
                    Capability::Summarization,
                    Capability::Code,
                    Capability::Reasoning,
                    Capability::Writing,
                    Capability::LongContext,
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
        routing: RoutingConfig {
            default_provider: ProviderId::Claude,
            code_provider: ProviderId::Codex,
            reasoning_provider: ProviderId::Claude,
            local_provider: ProviderId::Local,
        },
        rules: vec![
            RouteRule {
                name: "private-work-stays-local".to_string(),
                prefer: ProviderId::Local,
                when: RouteMatch {
                    private: Some(true),
                    ..RouteMatch::default()
                },
            },
            RouteRule {
                name: "small-simple-work-stays-local".to_string(),
                prefer: ProviderId::Local,
                when: RouteMatch {
                    task: Some(TaskHint::Simple),
                    max_input_tokens: Some(4_000),
                    ..RouteMatch::default()
                },
            },
            RouteRule {
                name: "long-context-to-gemini".to_string(),
                prefer: ProviderId::Gemini,
                when: RouteMatch {
                    long_context: Some(true),
                    ..RouteMatch::default()
                },
            },
            RouteRule {
                name: "repo-code-to-codex".to_string(),
                prefer: ProviderId::Codex,
                when: RouteMatch {
                    repo: Some(true),
                    ..RouteMatch::default()
                },
            },
        ],
        profiles: vec![
            ProjectProfile {
                name: "modelrouter".to_string(),
                path_contains: "Modelrouter".to_string(),
                default_provider: Some(ProviderId::Claude),
                code_provider: Some(ProviderId::Codex),
                reasoning_provider: Some(ProviderId::Claude),
                local_provider: Some(ProviderId::Local),
            },
            ProjectProfile {
                name: "miladapp".to_string(),
                path_contains: "MiladApp".to_string(),
                default_provider: Some(ProviderId::Claude),
                code_provider: Some(ProviderId::Codex),
                reasoning_provider: Some(ProviderId::Claude),
                local_provider: Some(ProviderId::Local),
            },
            ProjectProfile {
                name: "surveys".to_string(),
                path_contains: "Surveys".to_string(),
                default_provider: Some(ProviderId::Local),
                code_provider: Some(ProviderId::Codex),
                reasoning_provider: Some(ProviderId::Claude),
                local_provider: Some(ProviderId::Local),
            },
        ],
        favorites: detected_favorites(),
        classification: ClassificationConfig::default(),
        budget: crate::BudgetConfig {
            monthly_api_budget_cents: Some(5_000.0),
        },
        server: ServerConfig {
            auth_token: auth_token.map(str::to_string),
        },
    }
}

fn detected_favorites() -> Vec<PathFavorite> {
    let path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Current project")
        .to_string();
    vec![PathFavorite {
        name,
        path: path.display().to_string(),
    }]
}

fn maybe_classify_cli_request(
    config: &RouterConfig,
    mut request: RouteRequest,
    prompt: &str,
    has_preference: bool,
    has_hint: bool,
) -> (RouteRequest, Vec<String>) {
    let mut fallback_reasons = Vec::new();
    if config.classification.mode == crate::ClassificationMode::Llm && !has_preference && !has_hint
    {
        match classify_prompt_with_runner(config, prompt, |provider, classifier_prompt, cwd| {
            run_provider(provider, classifier_prompt, cwd).map_err(|error| error.to_string())
        }) {
            Ok(Some(classification)) => {
                request = request.with_classification(classification);
            }
            Ok(None) => {}
            Err(message) => fallback_reasons.push(format!(
                "LLM classifier unavailable: {message}; used heuristic classification."
            )),
        }
    }
    (request, fallback_reasons)
}

fn prepend_reasons(decision: &mut RouteDecision, fallback_reasons: Vec<String>) {
    if fallback_reasons.is_empty() {
        return;
    }
    let mut reasons = fallback_reasons;
    reasons.append(&mut decision.reasons);
    decision.reasons = reasons;
}

fn subscription_provider(
    id: ProviderId,
    model: &str,
    program: &str,
    max_input_tokens: u32,
    capabilities: Vec<Capability>,
) -> ProviderConfig {
    ProviderConfig {
        id,
        model: model.to_string(),
        enabled: command_available(program),
        kind: ProviderKind::SubscriptionCli,
        billing: BillingMode::Subscription,
        endpoint_url: None,
        input_cost_per_million_tokens: 0.0,
        output_cost_per_million_tokens: 0.0,
        max_input_tokens,
        capabilities,
    }
}

fn local_http_provider(
    id: ProviderId,
    model: &str,
    endpoint: &str,
    enabled: bool,
) -> ProviderConfig {
    ProviderConfig {
        id,
        model: model.to_string(),
        enabled,
        kind: ProviderKind::OpenAiCompatible,
        billing: BillingMode::Local,
        endpoint_url: Some(endpoint.to_string()),
        input_cost_per_million_tokens: 0.0,
        output_cost_per_million_tokens: 0.0,
        max_input_tokens: 128_000,
        capabilities: vec![
            Capability::Local,
            Capability::Privacy,
            Capability::Summarization,
            Capability::Code,
            Capability::Reasoning,
            Capability::Writing,
        ],
    }
}

fn run_doctor(config: Option<PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let path = resolved_config_path(config);
    let config_found = path.as_ref().is_some_and(|path| path.is_file());
    let config = match path.as_deref().filter(|path| path.is_file()) {
        Some(path) => load_config(path)?,
        None => RouterConfig::default(),
    };
    let providers = health_report(&config);
    let report = DoctorReport {
        config_path: path.as_ref().map(|path| path.display().to_string()),
        config_found,
        providers: providers.clone(),
        next_steps: doctor_next_steps(config_found, &providers),
    };

    if json_output {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!(
            "config: {}",
            report.config_path.as_deref().unwrap_or("not configured")
        );
        println!(
            "config_found: {}",
            if report.config_found { "yes" } else { "no" }
        );
        for provider in &report.providers {
            println!(
                "{}: {:?} ({}) {}",
                provider.provider, provider.status, provider.check, provider.message
            );
        }
        for step in &report.next_steps {
            println!("next: {step}");
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct DoctorReport {
    config_path: Option<String>,
    config_found: bool,
    providers: Vec<ProviderHealth>,
    next_steps: Vec<String>,
}

fn doctor_next_steps(config_found: bool, providers: &[ProviderHealth]) -> Vec<String> {
    let mut steps = Vec::new();
    if !config_found {
        steps.push("modelrouter init".to_string());
    }
    for provider in providers {
        if provider.enabled && provider.status != crate::ProviderHealthStatus::Available {
            steps.push(format!("fix {}: {}", provider.provider, provider.message));
        }
    }
    if steps.is_empty() {
        steps.push("modelrouter daemon start".to_string());
        steps.push("modelrouter route --prompt \"Summarize this note\" --json".to_string());
    }
    steps
}

fn run_daemon(command: DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Start {
            config,
            host,
            port,
            log,
            open,
        } => {
            let config_path = resolved_config_path(config.clone());
            let config = load_config_or_default(config)?;
            let log = log.unwrap_or_else(default_log_path);
            ensure_parent_dir(&log)?;
            let address = format!("{host}:{port}");
            let url = format!("http://{address}");
            eprintln!("modelrouter daemon on {url}");
            eprintln!("log: {}", log.display());
            if open {
                open_url(&url);
            }
            serve_with_log_and_config_path(config, address, Some(log), config_path)?;
        }
        DaemonCommand::LaunchdPlist {
            config,
            host,
            port,
            log,
            label,
            output,
        } => {
            let plist = launchd_plist(
                &label,
                &absolute_path(env::current_exe()?)?,
                &absolute_path(config.unwrap_or_else(default_config_path))?,
                &host,
                port,
                &absolute_path(log.unwrap_or_else(default_log_path))?,
            )?;
            if let Some(output) = output {
                ensure_parent_dir(&output)?;
                fs::write(&output, plist)?;
                println!("wrote {}", output.display());
            } else {
                println!("{plist}");
            }
        }
    }
    Ok(())
}

fn run_mcp_command(command: McpCommand, parent_config: Option<PathBuf>) -> anyhow::Result<()> {
    match command {
        McpCommand::InstallConfig {
            config,
            command,
            output,
        } => {
            let config = absolute_path(
                config
                    .or(parent_config)
                    .or_else(|| resolved_config_path(None))
                    .unwrap_or_else(default_config_path),
            )?;
            let command = absolute_path(command.unwrap_or(env::current_exe()?))?;
            let value = mcp_client_config(&command, &config);
            let contents = serde_json::to_string_pretty(&value)?;
            if let Some(output) = output {
                ensure_parent_dir(&output)?;
                fs::write(&output, contents)?;
                println!("wrote {}", output.display());
            } else {
                println!("{contents}");
            }
        }
    }
    Ok(())
}

fn mcp_client_config(command: &Path, config: &Path) -> Value {
    json!({
        "mcpServers": {
            "modelrouter": {
                "command": command.display().to_string(),
                "args": [
                    "mcp",
                    "--config",
                    config.display().to_string()
                ]
            }
        }
    })
}

fn launchd_plist(
    label: &str,
    command: &Path,
    config: &Path,
    host: &str,
    port: u16,
    log: &Path,
) -> anyhow::Result<String> {
    let cwd = env::current_dir()?;
    let stdout = log.with_file_name("modelrouter-launchd.out.log");
    let stderr = log.with_file_name("modelrouter-launchd.err.log");
    let path_env = env::var("PATH").unwrap_or_else(|_| {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()
    });
    let home_env = env::var("HOME").unwrap_or_default();
    let user_env = env::var("USER").unwrap_or_default();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>daemon</string>
    <string>start</string>
    <string>--config</string>
    <string>{}</string>
    <string>--host</string>
    <string>{}</string>
    <string>--port</string>
    <string>{}</string>
    <string>--log</string>
    <string>{}</string>
  </array>
  <key>WorkingDirectory</key>
  <string>{}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{}</string>
    <key>HOME</key>
    <string>{}</string>
    <key>USER</key>
    <string>{}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>"#,
        xml_escape(label),
        xml_escape(&command.display().to_string()),
        xml_escape(&config.display().to_string()),
        xml_escape(host),
        port,
        xml_escape(&log.display().to_string()),
        xml_escape(&cwd.display().to_string()),
        xml_escape(&path_env),
        xml_escape(&home_env),
        xml_escape(&user_env),
        xml_escape(&stdout.display().to_string()),
        xml_escape(&stderr.display().to_string()),
    ))
}

fn run_spend(command: SpendCommand) -> anyhow::Result<()> {
    match command {
        SpendCommand::Report { log, json } => {
            let metrics = load_metrics(&log)?;
            if json {
                println!("{}", serde_json::to_string(&metrics)?);
            } else {
                println!("total_requests: {}", metrics.total_requests);
                println!("successful_requests: {}", metrics.successful_requests);
                println!("failed_requests: {}", metrics.failed_requests);
                println!("estimated_cost_cents: {:.6}", metrics.estimated_cost_cents);
                for provider in metrics.by_provider {
                    println!(
                        "provider: {} requests={} cost_cents={:.6}",
                        provider.provider, provider.requests, provider.estimated_cost_cents
                    );
                }
            }
        }
        SpendCommand::OpenaiRequest {
            start_time,
            end_time,
            api_key_env,
        } => {
            let api_key = std::env::var(api_key_env)?;
            let request = build_openai_cost_request(
                SpendWindow {
                    start_unix_seconds: start_time,
                    end_unix_seconds: end_time,
                },
                &api_key,
            );
            println!("{}", serde_json::to_string(&request)?);
        }
        SpendCommand::AnthropicRequest {
            starting_at,
            ending_at,
            api_key_env,
        } => {
            let api_key = std::env::var(api_key_env)?;
            let request = build_anthropic_cost_request(&starting_at, &ending_at, &api_key);
            println!("{}", serde_json::to_string(&request)?);
        }
        SpendCommand::GoogleQuery {
            table,
            start_date,
            end_date,
        } => {
            println!(
                "{}",
                try_build_google_billing_query(&table, &start_date, &end_date)?
            );
        }
        SpendCommand::ParseOpenai { file } => {
            let body = std::fs::read_to_string(file)?;
            let report = parse_openai_costs_response(&body)?;
            println!("{}", serde_json::to_string(&report)?);
        }
        SpendCommand::ParseAnthropic { file } => {
            let body = std::fs::read_to_string(file)?;
            let report = parse_anthropic_cost_report(&body)?;
            println!("{}", serde_json::to_string(&report)?);
        }
        SpendCommand::SyncOpenai {
            start_time,
            end_time,
            api_key_env,
        } => {
            let api_key = std::env::var(api_key_env)?;
            let report = sync_openai_costs(
                SpendWindow {
                    start_unix_seconds: start_time,
                    end_unix_seconds: end_time,
                },
                &api_key,
            )?;
            println!("{}", serde_json::to_string(&report)?);
        }
        SpendCommand::SyncAnthropic {
            starting_at,
            ending_at,
            api_key_env,
        } => {
            let api_key = std::env::var(api_key_env)?;
            let report = sync_anthropic_cost_report(&starting_at, &ending_at, &api_key)?;
            println!("{}", serde_json::to_string(&report)?);
        }
    }
    Ok(())
}

fn append_log(
    log_path: Option<&std::path::Path>,
    operation: &str,
    prompt: &str,
    decision: &RouteDecision,
    start: Instant,
    success: bool,
    error: Option<String>,
) -> anyhow::Result<()> {
    if let Some(log_path) = log_path {
        let entry = RequestLogEntry::from_decision(
            operation,
            prompt,
            decision,
            start.elapsed().as_millis(),
            success,
            error,
        );
        append_request_log(log_path, &entry)?;
    }
    Ok(())
}

fn append_log_result(
    log_path: Option<&std::path::Path>,
    operation: &str,
    prompt: &str,
    decision: &RouteDecision,
    start: Instant,
    result: &crate::ProviderRunResult,
) -> anyhow::Result<()> {
    if let Some(log_path) = log_path {
        let entry = match result.usage {
            Some(usage) => RequestLogEntry::from_decision_with_actual_usage(
                operation,
                prompt,
                decision,
                usage,
                result.actual_cost_cents,
                start.elapsed().as_millis(),
            ),
            None => RequestLogEntry::from_decision(
                operation,
                prompt,
                decision,
                start.elapsed().as_millis(),
                true,
                None,
            ),
        };
        append_request_log(log_path, &entry)?;
    }
    Ok(())
}

fn config_for_cwd(config: RouterConfig, cwd: Option<&Path>) -> anyhow::Result<RouterConfig> {
    let cwd = match cwd {
        Some(cwd) => cwd.to_path_buf(),
        None => std::env::current_dir()?,
    };
    Ok(apply_project_profile(&config, &cwd))
}

fn load_config_or_default(config: Option<PathBuf>) -> anyhow::Result<RouterConfig> {
    match resolved_config_path(config).filter(|path| path.is_file()) {
        Some(path) => Ok(load_config(&path)?),
        None => Ok(RouterConfig::default()),
    }
}

fn resolved_config_path(config: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = config {
        return Some(path);
    }
    if let Ok(path) = env::var("MODELROUTER_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let path = default_config_path();
    path.is_file().then_some(path)
}

fn default_config_path() -> PathBuf {
    PathBuf::from("modelrouter.toml")
}

fn default_log_path() -> PathBuf {
    PathBuf::from(".modelrouter").join("requests.jsonl")
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn absolute_path(path: PathBuf) -> anyhow::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(env::current_dir()?.join(path))
}

fn command_available(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|path| path.join(program).is_file())
}

fn endpoint_available(endpoint: &str) -> bool {
    let base = endpoint.trim_end_matches('/');
    let models_url = if base.ends_with("/models") {
        base.to_string()
    } else {
        format!("{base}/models")
    };
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(700))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .and_then(|client| client.get(models_url).send())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn detect_openai_compatible_model(endpoint: &str) -> Option<String> {
    let base = endpoint.trim_end_matches('/');
    let models_url = if base.ends_with("/models") {
        base.to_string()
    } else {
        format!("{base}/models")
    };
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(700))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?
        .get(models_url)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let value = response.json::<Value>().ok()?;
    value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|models| {
            models.iter().find_map(|model| {
                model
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.contains("embed"))
                    .map(str::to_string)
            })
        })
        .or_else(|| {
            value
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| {
                    models.iter().find_map(|model| {
                        model
                            .get("name")
                            .or_else(|| model.get("model"))
                            .and_then(Value::as_str)
                            .filter(|id| !id.contains("embed"))
                            .map(str::to_string)
                    })
                })
        })
}

fn normalize_openai_base_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    if with_scheme.ends_with("/v1") {
        with_scheme
    } else {
        format!("{with_scheme}/v1")
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = ProcessCommand::new("open").arg(url).status();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = ProcessCommand::new("xdg-open").arg(url).status();
    }
    #[cfg(windows)]
    {
        let _ = ProcessCommand::new("cmd")
            .args(["/C", "start", "", url])
            .status();
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
