use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn route_command_prints_json_decision() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args(["route", "--prompt", "Summarize this quick note.", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""provider":"local""#))
        .stdout(predicate::str::contains(r#""estimated_cost_cents""#));
}

#[test]
fn route_command_rejects_unknown_preference() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args([
            "route",
            "--prompt",
            "Summarize this quick note.",
            "--prefer",
            "expensive-cloud",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid provider"));
}

#[test]
fn help_lists_run_command() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"));
}

#[test]
fn help_lists_serve_command() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"));
}

#[test]
fn help_lists_mcp_and_health_commands() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("mcp"))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("spend"));
}

#[test]
fn route_command_can_write_jsonl_log() {
    let dir = tempfile::tempdir().expect("temp dir");
    let log_path = dir.path().join("modelrouter.jsonl");
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args([
            "route",
            "--prompt",
            "Summarize this quick note.",
            "--json",
            "--log",
            log_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    let contents = std::fs::read_to_string(log_path).expect("log contents");
    assert!(contents.contains(r#""operation":"route""#));
    assert!(contents.ends_with('\n'));
}

#[test]
fn health_command_prints_json_report() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args(["health", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""providers""#));
}

#[test]
fn spend_report_reads_request_log() {
    let dir = tempfile::tempdir().expect("temp dir");
    let log_path = dir.path().join("modelrouter.jsonl");
    let mut route = Command::cargo_bin("modelrouter").expect("binary exists");
    route
        .args([
            "route",
            "--prompt",
            "Summarize this quick note.",
            "--log",
            log_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    let mut report = Command::cargo_bin("modelrouter").expect("binary exists");
    report
        .args([
            "spend",
            "report",
            "--log",
            log_path.to_str().expect("utf8 path"),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""total_requests":1"#));
}

#[test]
fn spend_google_query_prints_billing_export_query() {
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args([
            "spend",
            "google-query",
            "--table",
            "`billing.gcp_billing_export_v1_ABCDEF`",
            "--start-date",
            "2026-05-01",
            "--end-date",
            "2026-06-01",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("SUM(cost)"))
        .stdout(predicate::str::contains("Gemini"));
}
