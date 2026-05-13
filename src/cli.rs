use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    ProviderId, RequestLogEntry, RouteDecision, RouteRequest, Router, RouterConfig, SpendWindow,
    TaskHint, append_request_log, apply_project_profile, build_anthropic_cost_request,
    build_openai_cost_request, health_report, load_config, load_metrics,
    parse_anthropic_cost_report, parse_openai_costs_response, run_provider_with_usage, serve_mcp,
    serve_with_log, sync_anthropic_cost_report, sync_openai_costs, try_build_google_billing_query,
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Route AI tasks to local, Codex, or Claude models by cost and fit."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
    Mcp {
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
        Command::Route {
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
        } => {
            let config = match config {
                Some(path) => load_config(&path)?,
                None => RouterConfig::default(),
            };
            let config = config_for_cwd(config, cwd.as_deref())?;
            let router = Router::new(config);
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
            let decision = router.route(request)?;
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
        Command::Run {
            prompt,
            config,
            prefer,
            max_cost_cents,
            input_tokens,
            output_tokens,
            hint,
            cwd,
            log,
        } => {
            let config = match config {
                Some(path) => load_config(&path)?,
                None => RouterConfig::default(),
            };
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
            let decision = router.route(request)?;
            let provider = config.provider(decision.provider).ok_or(
                crate::ProviderRunError::MissingProvider {
                    provider: decision.provider,
                },
            )?;
            let output = run_provider_with_usage(provider, &prompt, cwd.as_deref())?;
            append_log_result(log.as_deref(), "run", &prompt, &decision, start, &output)?;
            println!("{}", output.output);
        }
        Command::Serve {
            config,
            host,
            port,
            log,
        } => {
            let config = match config {
                Some(path) => load_config(&path)?,
                None => RouterConfig::default(),
            };
            let address = format!("{host}:{port}");
            eprintln!("modelrouter serving on http://{address}");
            serve_with_log(config, address, log)?;
        }
        Command::Mcp { config } => {
            let config = match config {
                Some(path) => load_config(&path)?,
                None => RouterConfig::default(),
            };
            serve_mcp(config)?;
        }
        Command::Health { config, json } => {
            let config = match config {
                Some(path) => load_config(&path)?,
                None => RouterConfig::default(),
            };
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
        Command::Spend { command } => run_spend(command)?,
    }

    Ok(())
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
