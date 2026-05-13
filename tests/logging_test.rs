use modelrouter::{
    RequestLogEntry, RouteRequest, Router, RouterConfig, TokenUsage, append_request_log,
};

#[test]
fn appends_jsonl_request_log_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("router.jsonl");
    let entry = RequestLogEntry {
        timestamp_unix_ms: 123,
        operation: "route".to_string(),
        prompt_hash: "abc".to_string(),
        provider: "codex".to_string(),
        model: "auto".to_string(),
        billing: "subscription".to_string(),
        estimated_input_tokens: 10,
        estimated_output_tokens: 600,
        estimated_cost_cents: 0.0,
        actual_input_tokens: None,
        actual_output_tokens: None,
        actual_cost_cents: None,
        spend_source: None,
        latency_ms: 12,
        success: true,
        error: None,
        reasons: vec!["matched policy".to_string()],
    };

    append_request_log(&path, &entry).expect("append log");

    let contents = std::fs::read_to_string(path).expect("read log");
    assert!(contents.contains("\"operation\":\"route\""));
    assert!(contents.ends_with('\n'));
}

#[test]
fn request_log_can_include_actual_usage_and_cost() {
    let router = Router::new(RouterConfig::default());
    let decision = router
        .route(RouteRequest::new("Fix tests."))
        .expect("route should succeed");

    let entry = RequestLogEntry::from_decision_with_actual_usage(
        "run",
        "Fix tests.",
        &decision,
        TokenUsage {
            input_tokens: 12,
            output_tokens: 8,
            total_tokens: 20,
        },
        Some(3.5),
        20,
    );

    assert_eq!(entry.actual_input_tokens, Some(12));
    assert_eq!(entry.actual_output_tokens, Some(8));
    assert_eq!(entry.actual_cost_cents, Some(3.5));
    assert_eq!(entry.spend_source.as_deref(), Some("provider_response"));
}
