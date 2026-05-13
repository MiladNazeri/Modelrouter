use modelrouter::{
    ProviderId, RouteMatch, RouteRequest, RouteRule, Router, RouterConfig, TaskHint,
};

#[test]
fn route_rules_can_override_the_default_policy() {
    let mut config = RouterConfig::default();
    config.rules.push(RouteRule {
        name: "writing-to-gemini".to_string(),
        when: RouteMatch {
            task: Some(TaskHint::Writing),
            ..RouteMatch::default()
        },
        prefer: ProviderId::Gemini,
    });

    let decision = Router::new(config)
        .route(RouteRequest::new("Write a careful memo to the team."))
        .expect("route");

    assert_eq!(decision.provider, ProviderId::Gemini);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| { reason.contains("Matched routing rule writing-to-gemini") })
    );
}

#[test]
fn route_rules_can_match_private_requests() {
    let mut config = RouterConfig::default();
    config.rules.push(RouteRule {
        name: "private-claude-review".to_string(),
        when: RouteMatch {
            private: Some(true),
            ..RouteMatch::default()
        },
        prefer: ProviderId::Claude,
    });

    let decision = Router::new(config)
        .route(RouteRequest::new(
            "Summarize this private note but do not edit files.",
        ))
        .expect("route");

    assert_eq!(decision.provider, ProviderId::Claude);
}
