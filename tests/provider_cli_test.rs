use std::path::Path;

use modelrouter::{ProviderId, build_provider_command};

#[test]
fn builds_codex_subscription_cli_command() {
    let command = build_provider_command(
        ProviderId::Codex,
        "auto",
        "Fix the parser tests.",
        Some(Path::new("/tmp/project")),
    );

    assert_eq!(command.program, "codex");
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
        vec!["--output-format", "text", "--model", "gemini-2.5-pro"]
    );
    assert_eq!(command.stdin, "Analyze this long context.");
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
