use modelrouter::{
    BudgetConfig, BudgetState, RequestLogEntry, RouteRequest, Router, RouterConfig, SecretRef,
    SpendProvider, SpendWindow, build_anthropic_cost_request, build_google_billing_query,
    build_openai_cost_request, check_api_budget, parse_anthropic_cost_report,
    parse_google_billing_rows, parse_openai_costs_response, reconcile_spend,
};

#[test]
fn openai_cost_request_targets_organization_costs_endpoint() {
    let request = build_openai_cost_request(
        SpendWindow {
            start_unix_seconds: 1_779_676_800,
            end_unix_seconds: 1_782_268_800,
        },
        "OPENAI_ADMIN_KEY",
    );

    assert_eq!(request.method, "GET");
    assert!(request.url.contains("/v1/organization/costs"));
    assert!(request.url.contains("start_time=1779676800"));
    assert!(request.url.contains("end_time=1782268800"));
    assert!(request.headers.contains(&(
        "Authorization".to_string(),
        "Bearer OPENAI_ADMIN_KEY".to_string()
    )));
}

#[test]
fn parses_openai_costs_response_into_cents() {
    let report = parse_openai_costs_response(
        r#"{
          "data": [
            {
              "results": [
                {
                  "amount": { "value": 1.25, "currency": "usd" },
                  "line_item": "Text tokens",
                  "project_id": "proj_123"
                }
              ]
            }
          ]
        }"#,
    )
    .expect("report");

    assert_eq!(report.provider, SpendProvider::OpenAi);
    assert_eq!(report.total_cents, 125.0);
    assert_eq!(report.items[0].label, "Text tokens");
}

#[test]
fn anthropic_cost_request_targets_admin_cost_report() {
    let request = build_anthropic_cost_request(
        "2026-05-01T00:00:00Z",
        "2026-06-01T00:00:00Z",
        "ANTHROPIC_ADMIN_KEY",
    );

    assert_eq!(request.method, "GET");
    assert!(request.url.contains("/v1/organizations/cost_report"));
    assert!(request.url.contains("starting_at=2026-05-01T00%3A00%3A00Z"));
    assert!(
        request
            .headers
            .contains(&("anthropic-version".to_string(), "2023-06-01".to_string()))
    );
    assert!(
        request
            .headers
            .contains(&("x-api-key".to_string(), "ANTHROPIC_ADMIN_KEY".to_string()))
    );
}

#[test]
fn parses_anthropic_cost_report_cent_strings() {
    let report = parse_anthropic_cost_report(
        r#"{
          "data": [
            {
              "results": [
                {
                  "amount": "245.50",
                  "currency": "USD",
                  "description": "Claude messages",
                  "workspace_id": "ws_123"
                }
              ]
            }
          ]
        }"#,
    )
    .expect("report");

    assert_eq!(report.provider, SpendProvider::Anthropic);
    assert_eq!(report.total_cents, 245.5);
    assert_eq!(report.items[0].label, "Claude messages");
}

#[test]
fn google_billing_query_targets_gemini_cost_rows() {
    let query = build_google_billing_query(
        "`billing.gcp_billing_export_v1_ABCDEF`",
        "2026-05-01",
        "2026-06-01",
    );

    assert!(query.contains("SUM(cost)"));
    assert!(query.contains("usage_start_time"));
    assert!(query.contains("Gemini"));
}

#[test]
fn parses_google_billing_rows_as_dollar_costs() {
    let report = parse_google_billing_rows(
        r#"{
          "rows": [
            { "f": [
              { "v": "Gemini API" },
              { "v": "1.75" },
              { "v": "USD" }
            ] }
          ]
        }"#,
    )
    .expect("report");

    assert_eq!(report.provider, SpendProvider::Google);
    assert_eq!(report.total_cents, 175.0);
}

#[test]
fn budget_guardrail_denies_api_when_actual_spend_plus_projection_exceeds_limit() {
    let decision = check_api_budget(
        &BudgetConfig {
            monthly_api_budget_cents: Some(1_000.0),
        },
        &BudgetState {
            actual_monthly_api_spend_cents: 990.0,
        },
        20.0,
    );

    assert!(!decision.allowed);
    assert!(decision.reason.contains("monthly API budget"));
}

#[test]
fn reconciliation_compares_estimated_logs_to_actual_provider_spend() {
    let mut config = RouterConfig::default();
    config.routing.default_provider = modelrouter::ProviderId::OpenAiCompatible;
    config
        .providers
        .iter_mut()
        .find(|provider| provider.id == modelrouter::ProviderId::OpenAiCompatible)
        .expect("openai-compatible")
        .enabled = true;
    let decision = Router::new(config)
        .route(
            RouteRequest::new("Write a short memo.")
                .prefer(modelrouter::ProviderId::OpenAiCompatible),
        )
        .expect("decision");
    let logs = vec![RequestLogEntry::from_decision(
        "run",
        "Write a short memo.",
        &decision,
        10,
        true,
        None,
    )];
    let report = parse_openai_costs_response(
        r#"{"data":[{"results":[{"amount":{"value":0.42,"currency":"usd"},"line_item":"Text"}]}]}"#,
    )
    .expect("spend");

    let reconciliation = reconcile_spend(&logs, &[report]);

    assert_eq!(reconciliation.actual_total_cents, 42.0);
    assert!(reconciliation.delta_cents > 0.0);
}

#[test]
fn secret_refs_read_keys_from_environment() {
    temp_env::with_var("MODELROUTER_TEST_SECRET", Some("secret-value"), || {
        let secret = SecretRef::Env {
            name: "MODELROUTER_TEST_SECRET".to_string(),
        };

        assert_eq!(secret.resolve().as_deref(), Some("secret-value"));
    });
}
