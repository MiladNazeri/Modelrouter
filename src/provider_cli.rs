use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ProviderConfig, ProviderId, ProviderKind, RouteDecision, TokenUsage,
    run_http_provider_with_usage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCommand {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: String,
    pub working_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDiagnosticCategory {
    Auth,
    RateLimit,
    MissingCommand,
    ContextLimit,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDiagnostic {
    pub provider: String,
    pub category: ProviderDiagnosticCategory,
    pub message: String,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRunResult {
    pub output: String,
    pub usage: Option<TokenUsage>,
    pub actual_cost_cents: Option<f64>,
}

#[derive(Debug, Error)]
pub enum ProviderCliError {
    #[error("failed to start {program}: {source}")]
    Start {
        program: String,
        source: std::io::Error,
    },
    #[error("failed to write prompt to {program}: {source}")]
    Stdin {
        program: String,
        source: std::io::Error,
    },
    #[error("{program} failed with status {status}: {details}")]
    Failed {
        program: String,
        status: String,
        details: String,
    },
}

#[derive(Debug, Error)]
pub enum ProviderRunError {
    #[error(transparent)]
    Cli(#[from] ProviderCliError),
    #[error(transparent)]
    Http(#[from] crate::ProviderHttpError),
    #[error("{provider} is not configured")]
    MissingProvider { provider: ProviderId },
}

pub fn build_provider_command(
    provider: ProviderId,
    model: &str,
    prompt: &str,
    cwd: Option<&Path>,
) -> ProviderCommand {
    match provider {
        ProviderId::Codex => build_codex_command(model, prompt, cwd),
        ProviderId::Claude => build_claude_command(model, prompt, cwd),
        ProviderId::Gemini => build_gemini_command(model, prompt, cwd),
        ProviderId::Local => build_ollama_command(model, prompt, cwd),
        ProviderId::Aider => build_aider_command(model, prompt, cwd),
        ProviderId::LmStudio | ProviderId::LlamaCpp | ProviderId::OpenAiCompatible => {
            build_curl_placeholder_command(provider, model, prompt, cwd)
        }
    }
}

pub fn run_provider_command(command: &ProviderCommand) -> Result<String, ProviderCliError> {
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(working_dir) = &command.working_dir {
        process.current_dir(working_dir);
    }

    let mut child = process.spawn().map_err(|source| ProviderCliError::Start {
        program: command.program.clone(),
        source,
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(command.stdin.as_bytes())
            .map_err(|source| ProviderCliError::Stdin {
                program: command.program.clone(),
                source,
            })?;
    }

    let output = child
        .wait_with_output()
        .map_err(|source| ProviderCliError::Start {
            program: command.program.clone(),
            source,
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ProviderCliError::Failed {
            program: command.program.clone(),
            status: output.status.to_string(),
            details: command_failure_details(&stdout, &stderr),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn command_failure_details(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "no stdout or stderr captured".to_string(),
        (true, false) => format!("stderr: {stderr}"),
        (false, true) => format!("stdout: {stdout}"),
        (false, false) => format!("stderr: {stderr}\nstdout: {stdout}"),
    }
}

pub fn diagnose_provider_failure(provider: &str, output: &str) -> ProviderDiagnostic {
    let normalized = output.to_ascii_lowercase();
    let (category, action) = if normalized.contains("not logged in")
        || normalized.contains("login")
        || normalized.contains("unauthorized")
        || normalized.contains("authentication")
    {
        (
            ProviderDiagnosticCategory::Auth,
            format!("Run the {provider} login/auth command, then retry the request."),
        )
    } else if normalized.contains("rate limit") || normalized.contains("too many requests") {
        (
            ProviderDiagnosticCategory::RateLimit,
            format!("Wait for the {provider} quota window to reset or route to another provider."),
        )
    } else if normalized.contains("not found") || normalized.contains("no such file") {
        (
            ProviderDiagnosticCategory::MissingCommand,
            format!("Install the {provider} CLI and make sure it is on PATH."),
        )
    } else if normalized.contains("context") && normalized.contains("limit") {
        (
            ProviderDiagnosticCategory::ContextLimit,
            "Reduce the prompt size or enable compression before retrying.".to_string(),
        )
    } else {
        (
            ProviderDiagnosticCategory::Unknown,
            "Inspect the provider stderr and retry with a different provider if needed."
                .to_string(),
        )
    };

    ProviderDiagnostic {
        provider: provider.to_string(),
        category,
        message: output.trim().to_string(),
        action,
    }
}

pub fn run_decision(
    decision: &RouteDecision,
    prompt: &str,
    cwd: Option<&Path>,
) -> Result<String, ProviderCliError> {
    let command = build_provider_command(decision.provider, &decision.model, prompt, cwd);
    run_provider_command(&command)
}

pub fn run_provider(
    provider: &ProviderConfig,
    prompt: &str,
    cwd: Option<&Path>,
) -> Result<String, ProviderRunError> {
    Ok(run_provider_with_usage(provider, prompt, cwd)?.output)
}

pub fn run_provider_with_usage(
    provider: &ProviderConfig,
    prompt: &str,
    cwd: Option<&Path>,
) -> Result<ProviderRunResult, ProviderRunError> {
    match provider.kind {
        ProviderKind::SubscriptionCli | ProviderKind::LocalCli | ProviderKind::AgentCli => {
            let command = build_provider_command(provider.id, &provider.model, prompt, cwd);
            Ok(ProviderRunResult {
                output: run_provider_command(&command)?,
                usage: None,
                actual_cost_cents: None,
            })
        }
        ProviderKind::OpenAiCompatible => {
            let output = run_http_provider_with_usage(provider, prompt)?;
            let actual_cost_cents = output.usage.map(|usage| {
                provider.estimated_cost_cents(usage.input_tokens, usage.output_tokens)
            });
            Ok(ProviderRunResult {
                output: output.content,
                usage: output.usage,
                actual_cost_cents,
            })
        }
    }
}

fn build_codex_command(model: &str, prompt: &str, cwd: Option<&Path>) -> ProviderCommand {
    let mut args = vec![
        "exec".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    if let Some(cwd) = cwd {
        args.push("--cd".to_string());
        args.push(cwd.display().to_string());
    }
    push_model_args(&mut args, "--model", model);
    args.push("-".to_string());

    ProviderCommand {
        program: codex_program(),
        args,
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn codex_program() -> String {
    if let Ok(program) = env::var("MODELROUTER_CODEX_BIN")
        && !program.trim().is_empty()
    {
        return program;
    }

    let app_bundle_program = Path::new("/Applications/Codex.app/Contents/Resources/codex");
    if app_bundle_program.is_file() {
        return app_bundle_program.display().to_string();
    }

    "codex".to_string()
}

fn build_claude_command(model: &str, prompt: &str, cwd: Option<&Path>) -> ProviderCommand {
    let mut args = vec![
        "--print".to_string(),
        "--output-format".to_string(),
        "text".to_string(),
    ];
    push_model_args(&mut args, "--model", model);

    ProviderCommand {
        program: "claude".to_string(),
        args,
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn build_gemini_command(model: &str, prompt: &str, cwd: Option<&Path>) -> ProviderCommand {
    let mut args = vec!["--output-format".to_string(), "text".to_string()];
    push_model_args(&mut args, "--model", model);

    ProviderCommand {
        program: "gemini".to_string(),
        args,
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn build_ollama_command(model: &str, prompt: &str, cwd: Option<&Path>) -> ProviderCommand {
    ProviderCommand {
        program: "ollama".to_string(),
        args: vec!["run".to_string(), model.to_string()],
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn build_aider_command(model: &str, prompt: &str, cwd: Option<&Path>) -> ProviderCommand {
    let mut args = vec!["--yes".to_string()];
    push_model_args(&mut args, "--model", model);
    args.push("--message-file".to_string());
    args.push("/dev/stdin".to_string());

    ProviderCommand {
        program: "aider".to_string(),
        args,
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn build_curl_placeholder_command(
    provider: ProviderId,
    _model: &str,
    prompt: &str,
    cwd: Option<&Path>,
) -> ProviderCommand {
    ProviderCommand {
        program: format!("{provider}-http"),
        args: Vec::new(),
        stdin: prompt.to_string(),
        working_dir: cwd.map(Path::to_path_buf),
    }
}

fn push_model_args(args: &mut Vec<String>, flag: &str, model: &str) {
    if model.trim().is_empty() || model == "auto" {
        return;
    }
    args.push(flag.to_string());
    args.push(model.to_string());
}
