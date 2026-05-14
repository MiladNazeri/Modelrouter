use modelrouter::{
    ClassificationError, TaskHint, build_classification_prompt, parse_classification_response,
};

#[test]
fn parses_json_classifier_response() {
    let classification = parse_classification_response(
        r#"{"task":"code","private":false,"long_context":true,"repo":true,"confidence":0.87,"reason":"repo edits"}"#,
    )
    .expect("classification");

    assert_eq!(classification.task, Some(TaskHint::Code));
    assert_eq!(classification.private, Some(false));
    assert_eq!(classification.long_context, Some(true));
    assert_eq!(classification.repo, Some(true));
    assert_eq!(classification.confidence, Some(0.87));
    assert_eq!(classification.reason.as_deref(), Some("repo edits"));
}

#[test]
fn extracts_json_from_wrapped_classifier_response() {
    let classification = parse_classification_response(
        "Sure: {\"task\":\"writing\",\"private\":true,\"long_context\":false,\"repo\":false}",
    )
    .expect("classification");

    assert_eq!(classification.task, Some(TaskHint::Writing));
    assert_eq!(classification.private, Some(true));
}

#[test]
fn rejects_classifier_response_without_json() {
    let error = parse_classification_response("code request").expect_err("error");

    assert_eq!(error, ClassificationError::MissingJson);
}

#[test]
fn classifier_prompt_truncates_long_requests() {
    let prompt = build_classification_prompt(&"a".repeat(20), 8);

    assert!(prompt.contains("aaaaaaaa"));
    assert!(prompt.contains("[truncated]"));
}
