use modelrouter::{ProviderDiagnosticCategory, diagnose_provider_failure};

#[test]
fn provider_diagnostics_classify_auth_failures() {
    let diagnostic = diagnose_provider_failure("claude", "not logged in; please login");

    assert_eq!(diagnostic.category, ProviderDiagnosticCategory::Auth);
    assert!(diagnostic.action.contains("claude"));
}

#[test]
fn provider_diagnostics_classify_rate_limits() {
    let diagnostic = diagnose_provider_failure("codex", "rate limit exceeded");

    assert_eq!(diagnostic.category, ProviderDiagnosticCategory::RateLimit);
}
