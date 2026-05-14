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
        .stdout(predicate::str::contains("tui"))
        .stdout(predicate::str::contains("chat"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("daemon"))
        .stdout(predicate::str::contains("mcp"))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("spend"));
}

#[test]
fn init_command_writes_safe_local_config() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .current_dir(dir.path())
        .args([
            "init",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--local-endpoint",
            "http://localhost:11434/v1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote"));

    let contents = std::fs::read_to_string(&config_path).expect("config contents");
    assert!(contents.contains("[[providers]]"));
    assert!(contents.contains("id = \"codex\""));
    assert!(contents.contains("id = \"claude\""));
    assert!(contents.contains("id = \"gemini\""));
    assert!(contents.contains("endpoint_url = \"http://localhost:11434/v1\""));
    assert!(contents.contains("[[favorites]]"));
    assert!(contents.contains(dir.path().to_str().expect("utf8 path")));
    assert!(!contents.contains("sk-"));
}

#[test]
fn init_command_refuses_to_overwrite_without_force() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    std::fs::write(&config_path, "existing = true\n").expect("write config");
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .current_dir(dir.path())
        .args(["init", "--config", config_path.to_str().expect("utf8 path")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn doctor_command_reports_config_and_providers_as_json() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    let mut init = Command::cargo_bin("modelrouter").expect("binary exists");
    init.current_dir(dir.path())
        .args([
            "init",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--force",
        ])
        .assert()
        .success();
    let mut doctor = Command::cargo_bin("modelrouter").expect("binary exists");

    doctor
        .current_dir(dir.path())
        .args([
            "doctor",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""config_found":true"#))
        .stdout(predicate::str::contains(r#""providers""#))
        .stdout(predicate::str::contains(r#""next_steps""#));
}

#[test]
fn daemon_launchd_plist_prints_launch_agent() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    std::fs::write(&config_path, "# local config\n").expect("write config");
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args([
            "daemon",
            "launchd-plist",
            "--config",
            config_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("com.modelrouter.daemon"))
        .stdout(predicate::str::contains("ProgramArguments"))
        .stdout(predicate::str::contains("EnvironmentVariables"))
        .stdout(predicate::str::contains("<key>PATH</key>"))
        .stdout(predicate::str::contains("daemon"))
        .stdout(predicate::str::contains("start"));
}

#[test]
fn mcp_install_config_prints_client_json() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("modelrouter.toml");
    std::fs::write(&config_path, "# local config\n").expect("write config");
    let mut command = Command::cargo_bin("modelrouter").expect("binary exists");

    command
        .args([
            "mcp",
            "install-config",
            "--config",
            config_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""mcpServers""#))
        .stdout(predicate::str::contains(r#""modelrouter""#))
        .stdout(predicate::str::contains(r#""mcp""#));
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
