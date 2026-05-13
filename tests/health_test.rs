use modelrouter::{
    ProviderHealthStatus, ProviderId, RouterConfig, availability_filtered_config,
    health_report_with,
};

#[test]
fn reports_cli_and_http_provider_health() {
    let config = RouterConfig::default();
    let report = health_report_with(
        &config,
        |program| matches!(program, "codex" | "claude" | "gemini" | "ollama"),
        |url| url == "http://localhost:1234/v1",
    );

    let gemini = report
        .iter()
        .find(|item| item.provider == ProviderId::Gemini)
        .expect("gemini health");
    assert_eq!(gemini.status, ProviderHealthStatus::Available);

    let lmstudio = report
        .iter()
        .find(|item| item.provider == ProviderId::LmStudio)
        .expect("lmstudio health");
    assert_eq!(lmstudio.status, ProviderHealthStatus::Disabled);
}

#[test]
fn availability_filter_disables_missing_enabled_providers() {
    let config = RouterConfig::default();
    let report = health_report_with(&config, |program| program != "gemini", |_url| false);

    let filtered = availability_filtered_config(&config, &report);

    assert!(
        !filtered
            .provider(ProviderId::Gemini)
            .expect("gemini provider")
            .enabled
    );
    assert!(
        filtered
            .provider(ProviderId::Claude)
            .expect("claude provider")
            .enabled
    );
}
