mod cli;
mod compression;
mod config;
mod feedback;
mod gui;
mod health;
mod logging;
mod mcp;
mod metrics;
mod provider_cli;
mod provider_http;
mod queue;
mod router;
mod server;
mod spend;
mod tui;

pub use compression::{CompressedPrompt, compress_prompt};
pub use config::{
    BillingMode, Capability, ProjectProfile, ProviderConfig, ProviderId, ProviderKind, RouteMatch,
    RouteRule, RouterConfig, RoutingConfig, ServerConfig, TaskHint, apply_project_profile,
    load_config,
};
pub use feedback::{FeedbackEntry, append_feedback};
pub use gui::GUI_HTML;
pub use health::{
    ProviderHealth, ProviderHealthStatus, availability_filtered_config, health_report,
    health_report_with,
};
pub use logging::{RequestLogEntry, append_request_log, prompt_hash};
pub use mcp::{handle_mcp_message, handle_mcp_message_with_runner, serve_mcp};
pub use metrics::{ProviderMetrics, RouterMetrics, load_metrics, metrics_from_logs};
pub use provider_cli::{
    ProviderCliError, ProviderCommand, ProviderDiagnostic, ProviderDiagnosticCategory,
    ProviderRunError, ProviderRunResult, build_provider_command, diagnose_provider_failure,
    run_decision, run_provider, run_provider_command, run_provider_with_usage,
};
pub use provider_http::{
    HttpProviderOutput, HttpProviderRequest, ProviderHttpError, TokenUsage,
    build_http_provider_request, parse_openai_compatible_response,
    parse_openai_compatible_response_with_usage, run_http_provider, run_http_provider_with_usage,
};
pub use queue::{ExecutionQueue, QueueJob, QueueStatus};
pub use router::{RouteDecision, RouteRequest, Router, RouterError};
pub use server::{
    MAX_REQUEST_BODY_BYTES, ServerResponse, handle_server_request,
    handle_server_request_with_config_path, handle_server_request_with_config_path_and_runner,
    handle_server_request_with_headers, handle_server_request_with_headers_and_runner,
    handle_server_request_with_runner, handle_server_request_with_runner_and_report, serve,
    serve_with_log, serve_with_log_and_config_path,
};
pub use spend::{
    BudgetConfig, BudgetDecision, BudgetState, ProviderSpendReport, SecretRef, SpendHttpRequest,
    SpendInputError, SpendItem, SpendProvider, SpendReconciliation, SpendSyncError, SpendWindow,
    build_anthropic_cost_request, build_google_billing_query, build_openai_cost_request,
    check_api_budget, execute_spend_request, parse_anthropic_cost_report,
    parse_google_billing_rows, parse_openai_costs_response, reconcile_spend,
    sync_anthropic_cost_report, sync_openai_costs, try_build_google_billing_query,
};
pub use tui::{TuiEvent, TuiState, TuiView, run_tui};

pub fn run_cli() -> anyhow::Result<()> {
    cli::run()
}
