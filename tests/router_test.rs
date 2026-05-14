use modelrouter::{
    Capability, ProviderId, RouteClassification, RouteRequest, Router, RouterConfig, RouterError,
    TaskHint,
};

#[test]
fn sends_small_low_risk_prompts_to_local_model() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(RouteRequest::new(
            "Summarize this short note into three bullets.",
        ))
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Local);
    assert_eq!(decision.model, "llama3.2:3b");
    assert!(
        decision.estimated_cost_cents <= 0.01,
        "local route should be effectively free"
    );
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("low-risk"))
    );
}

#[test]
fn keeps_private_summarization_local_when_it_fits_context() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(RouteRequest::new(
            "Summarize this private journal entry and keep it local.",
        ))
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Local);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("privacy"))
    );
}

#[test]
fn routes_codebase_work_to_codex() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(
            RouteRequest::new("Fix the failing Rust tests and update the CLI parser.")
                .with_hint(TaskHint::Code),
        )
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Codex);
    assert!(decision.capabilities.contains(&Capability::CodebaseEditing));
    assert_eq!(
        decision.estimated_cost_cents, 0.0,
        "subscription-backed Codex should not report API spend"
    );
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("Scored"))
    );
}

#[test]
fn infers_codebase_editing_without_an_explicit_hint() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(RouteRequest::new(
            "In this repo, edit the Rust CLI parser, fix the failing tests, and update docs.",
        ))
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Codex);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("codebase editing"))
    );
}

#[test]
fn llm_classification_can_supply_task_signals() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(
            RouteRequest::new("Please handle this.").with_classification(RouteClassification {
                task: Some(TaskHint::Code),
                repo: Some(true),
                confidence: Some(0.91),
                reason: Some("Needs repo edits.".to_string()),
                ..RouteClassification::default()
            }),
        )
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Codex);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("LLM classifier supplied routing signals"))
    );
}

#[test]
fn sends_long_context_analysis_to_long_context_reasoning_model() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(
            RouteRequest::new("Analyze this product strategy and architecture dossier.")
                .with_estimated_input_tokens(150_000)
                .with_estimated_output_tokens(4_000),
        )
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Claude);
    assert!(decision.capabilities.contains(&Capability::LongContext));
}

#[test]
fn explicit_preference_wins_when_budget_allows_it() {
    let router = Router::new(RouterConfig::default());

    let decision = router
        .route(
            RouteRequest::new("Write a careful product strategy memo.")
                .prefer(ProviderId::Claude)
                .with_max_cost_cents(20.0),
        )
        .expect("route should succeed");

    assert_eq!(decision.provider, ProviderId::Claude);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("explicit preference"))
    );
}

#[test]
fn disabled_providers_are_not_selected() {
    let mut config = RouterConfig::default();
    config.disable_provider(ProviderId::Codex);
    let router = Router::new(config);

    let decision = router
        .route(RouteRequest::new("Refactor this CLI and add tests.").with_hint(TaskHint::Code))
        .expect("route should still choose a fallback");

    assert_ne!(decision.provider, ProviderId::Codex);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("Codex unavailable"))
    );
}

#[test]
fn hard_budget_returns_error_when_no_provider_can_fit() {
    let router = Router::new(RouterConfig::default());

    let error = router
        .route(
            RouteRequest::new("Analyze this very long legal and architecture document.")
                .with_hint(TaskHint::DeepReasoning)
                .with_estimated_input_tokens(1_200_000)
                .with_estimated_output_tokens(12_000)
                .with_max_cost_cents(0.001),
        )
        .expect_err("route should fail when no provider has enough context");

    assert!(matches!(error, RouterError::NoAffordableProvider { .. }));
}

#[test]
fn monthly_api_budget_blocks_api_provider_when_projected_spend_exceeds_limit() {
    let mut config = RouterConfig::default();
    for provider in &mut config.providers {
        provider.enabled = provider.id == ProviderId::OpenAiCompatible;
        if provider.id == ProviderId::OpenAiCompatible {
            provider.endpoint_url = Some("http://localhost:8000/v1".to_string());
            provider.input_cost_per_million_tokens = 1_000_000.0;
            provider.output_cost_per_million_tokens = 1_000_000.0;
        }
    }
    config.routing.default_provider = ProviderId::OpenAiCompatible;
    config.routing.code_provider = ProviderId::OpenAiCompatible;
    config.routing.reasoning_provider = ProviderId::OpenAiCompatible;
    config.routing.local_provider = ProviderId::OpenAiCompatible;
    config.budget.monthly_api_budget_cents = Some(1_000.0);

    let error = Router::new(config)
        .route(
            RouteRequest::new("Analyze this.")
                .prefer(ProviderId::OpenAiCompatible)
                .with_actual_monthly_api_spend_cents(990.0)
                .with_estimated_input_tokens(20)
                .with_estimated_output_tokens(20),
        )
        .expect_err("budget should block API provider");

    assert!(matches!(error, RouterError::ProviderUnavailable { .. }));
}
