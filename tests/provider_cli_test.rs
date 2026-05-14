use std::path::Path;

use modelrouter::{ProviderCommand, ProviderId, build_provider_command, run_provider_command};

#[test]
fn builds_codex_subscription_cli_command() {
    let command = build_provider_command(
        ProviderId::Codex,
        "auto",
        "Fix the parser tests.",
        Some(Path::new("/tmp/project")),
    );

    assert_eq!(
        Path::new(&command.program)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("codex")
    );
    assert_eq!(
        command.args,
        vec!["exec", "--color", "never", "--cd", "/tmp/project", "-"]
    );
    assert_eq!(command.stdin, "Fix the parser tests.");
}

#[test]
fn builds_claude_subscription_cli_command() {
    let command = build_provider_command(
        ProviderId::Claude,
        "sonnet",
        "Analyze this architecture memo.",
        Some(Path::new("/tmp/project")),
    );

    assert_eq!(command.program, "claude");
    assert_eq!(
        command.args,
        vec!["--print", "--output-format", "text", "--model", "sonnet"]
    );
    assert_eq!(
        command.working_dir.as_deref(),
        Some(Path::new("/tmp/project"))
    );
    assert_eq!(command.stdin, "Analyze this architecture memo.");
}

#[test]
fn builds_ollama_local_command() {
    let command = build_provider_command(
        ProviderId::Local,
        "llama3.2:3b",
        "Summarize this note.",
        None,
    );

    assert_eq!(command.program, "ollama");
    assert_eq!(command.args, vec!["run", "llama3.2:3b"]);
    assert_eq!(command.stdin, "Summarize this note.");
}

#[test]
fn builds_gemini_subscription_cli_command() {
    let command = build_provider_command(
        ProviderId::Gemini,
        "gemini-2.5-pro",
        "Analyze this long context.",
        None,
    );

    assert_eq!(command.program, "gemini");
    assert_eq!(
        command.args,
        vec![
            "--model",
            "gemini-2.5-pro",
            "--prompt",
            "Analyze this long context."
        ]
    );
    assert_eq!(command.stdin, "");
}

#[test]
fn builds_aider_agent_cli_command() {
    let command = build_provider_command(
        ProviderId::Aider,
        "auto",
        "Refactor this module.",
        Some(Path::new("/tmp/project")),
    );

    assert_eq!(command.program, "aider");
    assert_eq!(command.args, vec!["--yes", "--message-file", "/dev/stdin"]);
    assert_eq!(command.stdin, "Refactor this module.");
    assert_eq!(
        command.working_dir.as_deref(),
        Some(Path::new("/tmp/project"))
    );
}

#[test]
fn failed_provider_command_includes_stdout_when_stderr_is_empty() {
    let command = ProviderCommand {
        program: "/bin/sh".to_string(),
        args: vec![
            "-c".to_string(),
            "printf 'codex refused this request'; exit 1".to_string(),
        ],
        stdin: String::new(),
        working_dir: None,
    };

    let error = run_provider_command(&command).expect_err("command should fail");
    let message = error.to_string();

    assert!(message.contains("exit status: 1"));
    assert!(message.contains("stdout: codex refused this request"));
}
