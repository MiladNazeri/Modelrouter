use modelrouter::compress_prompt;

#[test]
fn compression_preserves_edges_and_reports_savings() {
    let prompt = format!(
        "Header context\n{}\nAction: fix tests\nTag: code",
        "middle detail ".repeat(200)
    );

    let compressed = compress_prompt(&prompt, 240);

    assert!(compressed.prompt.len() <= 240);
    assert!(compressed.prompt.contains("Header context"));
    assert!(compressed.prompt.contains("Action: fix tests"));
    assert!(compressed.omitted_chars > 0);
    assert!(compressed.estimated_saved_tokens > 0);
}
