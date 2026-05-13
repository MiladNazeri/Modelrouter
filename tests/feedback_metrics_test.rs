use std::path::Path;

use modelrouter::{
    FeedbackEntry, RequestLogEntry, RouteRequest, Router, RouterConfig, append_feedback,
    metrics_from_logs,
};

#[test]
fn feedback_entries_are_written_as_jsonl() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("feedback.jsonl");

    append_feedback(
        &path,
        &FeedbackEntry {
            route_id: "abc".to_string(),
            provider: "codex".to_string(),
            rating: "good".to_string(),
            note: Some("right call".to_string()),
        },
    )
    .expect("write feedback");

    let contents = std::fs::read_to_string(path).expect("feedback");
    assert!(contents.contains(r#""rating":"good""#));
    assert!(contents.ends_with('\n'));
}

#[test]
fn metrics_roll_up_provider_usage_and_cost() {
    let decision = Router::new(RouterConfig::default())
        .route(RouteRequest::new("Fix the failing Rust tests."))
        .expect("decision");
    let logs = vec![RequestLogEntry::from_decision(
        "run",
        "Fix the failing Rust tests.",
        &decision,
        123,
        true,
        None,
    )];

    let metrics = metrics_from_logs(&logs);

    assert_eq!(metrics.total_requests, 1);
    assert_eq!(metrics.by_provider[0].provider, "codex");
    assert_eq!(metrics.by_provider[0].requests, 1);
}

#[test]
fn metrics_endpoint_reads_jsonl_logs() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("requests.jsonl");
    let decision = Router::new(RouterConfig::default())
        .route(RouteRequest::new("Fix the failing Rust tests."))
        .expect("decision");
    let entry = RequestLogEntry::from_decision("route", "Fix tests", &decision, 1, true, None);
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&entry).expect("json")),
    )
    .expect("write log");

    let metrics = modelrouter::load_metrics(Path::new(&path)).expect("metrics");

    assert_eq!(metrics.total_requests, 1);
}
