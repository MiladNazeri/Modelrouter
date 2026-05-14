use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{
    BillingMode, Capability, ProviderConfig, ProviderId, RouteRule, RouterConfig, TaskHint,
};

#[derive(Clone, Debug)]
struct TaskProfile {
    hint: TaskHint,
    codebase_editing: bool,
    tests: bool,
    privacy_sensitive: bool,
    long_context: bool,
    low_risk: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RouteClassification {
    #[serde(default)]
    pub task: Option<TaskHint>,
    #[serde(default)]
    pub private: Option<bool>,
    #[serde(default)]
    pub long_context: Option<bool>,
    #[serde(default)]
    pub repo: Option<bool>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
struct ScoredProvider<'a> {
    provider: &'a ProviderConfig,
    score: f64,
    reasons: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RouteRequest {
    prompt: String,
    preferred_provider: Option<ProviderId>,
    max_cost_cents: Option<f64>,
    estimated_input_tokens: Option<u32>,
    estimated_output_tokens: Option<u32>,
    hint: Option<TaskHint>,
    classification: Option<RouteClassification>,
    actual_monthly_api_spend_cents: Option<f64>,
}

impl RouteRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            preferred_provider: None,
            max_cost_cents: None,
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            hint: None,
            classification: None,
            actual_monthly_api_spend_cents: None,
        }
    }

    pub fn prefer(mut self, provider: ProviderId) -> Self {
        self.preferred_provider = Some(provider);
        self
    }

    pub fn with_max_cost_cents(mut self, max_cost_cents: f64) -> Self {
        self.max_cost_cents = Some(max_cost_cents);
        self
    }

    pub fn with_estimated_input_tokens(mut self, tokens: u32) -> Self {
        self.estimated_input_tokens = Some(tokens);
        self
    }

    pub fn with_estimated_output_tokens(mut self, tokens: u32) -> Self {
        self.estimated_output_tokens = Some(tokens);
        self
    }

    pub fn with_hint(mut self, hint: TaskHint) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn with_classification(mut self, classification: RouteClassification) -> Self {
        self.classification = Some(classification);
        self
    }

    pub fn with_actual_monthly_api_spend_cents(mut self, spend_cents: f64) -> Self {
        self.actual_monthly_api_spend_cents = Some(spend_cents);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub provider: ProviderId,
    pub model: String,
    pub billing: BillingMode,
    pub estimated_input_tokens: u32,
    pub estimated_output_tokens: u32,
    pub estimated_cost_cents: f64,
    pub capabilities: Vec<Capability>,
    pub confidence: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("config file read failed at {path}: {source}")]
    ConfigIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config parse failed: {source}")]
    ConfigParse { source: toml::de::Error },
    #[error("{0}")]
    InvalidConfig(String),
    #[error("{provider} is unavailable for this route")]
    ProviderUnavailable { provider: ProviderId },
    #[error(
        "no affordable provider for {estimated_input_tokens} input tokens and {estimated_output_tokens} output tokens under {max_cost_cents} cents"
    )]
    NoAffordableProvider {
        max_cost_cents: f64,
        estimated_input_tokens: u32,
        estimated_output_tokens: u32,
    },
}

pub struct Router {
    config: RouterConfig,
}

impl Router {
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    pub fn route(&self, request: RouteRequest) -> Result<RouteDecision, RouterError> {
        self.config.validate()?;

        let input_tokens = request
            .estimated_input_tokens
            .unwrap_or_else(|| estimate_tokens(&request.prompt));
        let output_tokens = request.estimated_output_tokens.unwrap_or(600);
        let profile = task_profile(
            &request.prompt,
            input_tokens,
            request.hint,
            request.classification.as_ref(),
        );
        let classification_reasons = classification_reasons(request.classification.as_ref());

        if let Some(provider_id) = request.preferred_provider {
            let provider = self
                .eligible_provider(
                    provider_id,
                    input_tokens,
                    output_tokens,
                    request.max_cost_cents,
                    request.actual_monthly_api_spend_cents,
                )
                .ok_or(RouterError::ProviderUnavailable {
                    provider: provider_id,
                })?;
            return Ok(self.decision(
                provider,
                input_tokens,
                output_tokens,
                0.98,
                vec![format!(
                    "Honored explicit preference for {}.",
                    provider.id.label()
                )],
            ));
        }

        if let Some(rule) = self.matching_rule(&profile, input_tokens)
            && let Some(provider) = self.eligible_provider(
                rule.prefer,
                input_tokens,
                output_tokens,
                request.max_cost_cents,
                request.actual_monthly_api_spend_cents,
            )
        {
            let mut reasons = classification_reasons.clone();
            reasons.push(format!(
                "Matched routing rule {}; preferred {}.",
                rule.name,
                provider.id.label()
            ));
            return Ok(self.decision(provider, input_tokens, output_tokens, 0.96, reasons));
        }

        let scored = self.scored_candidates(
            &profile,
            input_tokens,
            output_tokens,
            request.max_cost_cents,
            request.actual_monthly_api_spend_cents,
        );
        if let Some(best) = scored
            .iter()
            .max_by(|left, right| left.score.total_cmp(&right.score))
        {
            let mut reasons = vec![
                "Scored eligible providers by task fit, cost, context capacity, and routing policy."
                    .to_string(),
            ];
            reasons.extend(classification_reasons);
            if let Some(reason) = self.unavailable_preferred_reason(
                &profile,
                best.provider.id,
                input_tokens,
                output_tokens,
                request.max_cost_cents,
                request.actual_monthly_api_spend_cents,
            ) {
                reasons.push(reason);
            }
            reasons.extend(best.reasons.clone());
            return Ok(self.decision(
                best.provider,
                input_tokens,
                output_tokens,
                confidence_from_score(best.score),
                reasons,
            ));
        }

        Err(RouterError::NoAffordableProvider {
            max_cost_cents: request.max_cost_cents.unwrap_or(0.0),
            estimated_input_tokens: input_tokens,
            estimated_output_tokens: output_tokens,
        })
    }

    fn eligible_provider(
        &self,
        provider_id: ProviderId,
        input_tokens: u32,
        output_tokens: u32,
        max_cost_cents: Option<f64>,
        actual_monthly_api_spend_cents: Option<f64>,
    ) -> Option<&ProviderConfig> {
        let provider = self.config.provider(provider_id)?;
        if !provider.enabled || input_tokens > provider.max_input_tokens {
            return None;
        }
        let cost = provider.estimated_cost_cents(input_tokens, output_tokens);
        if max_cost_cents.is_some_and(|budget| cost > budget) {
            return None;
        }
        if provider.billing == BillingMode::Api
            && let Some(limit) = self.config.budget.monthly_api_budget_cents
            && actual_monthly_api_spend_cents.unwrap_or_default() + cost > limit
        {
            return None;
        }
        Some(provider)
    }

    fn scored_candidates(
        &self,
        profile: &TaskProfile,
        input_tokens: u32,
        output_tokens: u32,
        max_cost_cents: Option<f64>,
        actual_monthly_api_spend_cents: Option<f64>,
    ) -> Vec<ScoredProvider<'_>> {
        let preferred = self.policy_provider(profile);
        let mut candidates: Vec<ScoredProvider<'_>> = self
            .config
            .providers
            .iter()
            .filter(|provider| {
                provider.enabled
                    && input_tokens <= provider.max_input_tokens
                    && supports_required_capability(provider, profile)
            })
            .filter(|provider| {
                max_cost_cents.is_none_or(|budget| {
                    provider.estimated_cost_cents(input_tokens, output_tokens) <= budget
                })
            })
            .filter(|provider| {
                provider.billing != BillingMode::Api
                    || self
                        .config
                        .budget
                        .monthly_api_budget_cents
                        .is_none_or(|limit| {
                            actual_monthly_api_spend_cents.unwrap_or_default()
                                + provider.estimated_cost_cents(input_tokens, output_tokens)
                                <= limit
                        })
            })
            .map(|provider| {
                score_provider(provider, profile, preferred, input_tokens, output_tokens)
            })
            .collect();

        if candidates.is_empty() {
            return candidates;
        }

        let cheapest = candidates
            .iter()
            .map(|candidate| {
                candidate
                    .provider
                    .estimated_cost_cents(input_tokens, output_tokens)
            })
            .min_by(f64::total_cmp)
            .unwrap_or(0.0);

        for candidate in &mut candidates {
            let cost = candidate
                .provider
                .estimated_cost_cents(input_tokens, output_tokens);
            if cost == cheapest {
                candidate.score += 0.18;
                candidate
                    .reasons
                    .push("Lowest estimated cost among capable providers.".to_string());
            } else if cost <= cheapest + 0.01 {
                candidate.score += 0.08;
            }
        }

        candidates
    }

    fn policy_provider(&self, profile: &TaskProfile) -> ProviderId {
        match profile.hint {
            TaskHint::Code => self.config.routing.code_provider,
            TaskHint::DeepReasoning | TaskHint::Writing => self.config.routing.reasoning_provider,
            TaskHint::Simple => {
                if profile.low_risk || profile.privacy_sensitive {
                    self.config.routing.local_provider
                } else {
                    self.config.routing.default_provider
                }
            }
        }
    }

    fn unavailable_preferred_reason(
        &self,
        profile: &TaskProfile,
        selected: ProviderId,
        input_tokens: u32,
        output_tokens: u32,
        max_cost_cents: Option<f64>,
        actual_monthly_api_spend_cents: Option<f64>,
    ) -> Option<String> {
        let preferred = self.policy_provider(profile);
        if preferred == selected {
            return None;
        }
        if self
            .eligible_provider(
                preferred,
                input_tokens,
                output_tokens,
                max_cost_cents,
                actual_monthly_api_spend_cents,
            )
            .is_none()
        {
            return Some(format!(
                "{} unavailable; using the highest-scored affordable fallback.",
                preferred.label()
            ));
        }
        None
    }

    fn matching_rule(&self, profile: &TaskProfile, input_tokens: u32) -> Option<&RouteRule> {
        self.config
            .rules
            .iter()
            .find(|rule| route_rule_matches(rule, profile, input_tokens))
    }

    fn decision(
        &self,
        provider: &ProviderConfig,
        input_tokens: u32,
        output_tokens: u32,
        confidence: f32,
        reasons: Vec<String>,
    ) -> RouteDecision {
        RouteDecision {
            provider: provider.id,
            model: provider.model.clone(),
            billing: provider.billing,
            estimated_input_tokens: input_tokens,
            estimated_output_tokens: output_tokens,
            estimated_cost_cents: provider.estimated_cost_cents(input_tokens, output_tokens),
            capabilities: provider.capabilities.clone(),
            confidence,
            reasons,
        }
    }
}

fn route_rule_matches(rule: &RouteRule, profile: &TaskProfile, input_tokens: u32) -> bool {
    if rule.when.task.is_some_and(|task| task != profile.hint) {
        return false;
    }
    if rule
        .when
        .private
        .is_some_and(|expected| expected != profile.privacy_sensitive)
    {
        return false;
    }
    if rule
        .when
        .long_context
        .is_some_and(|expected| expected != profile.long_context)
    {
        return false;
    }
    if rule
        .when
        .repo
        .is_some_and(|expected| expected != profile.codebase_editing)
    {
        return false;
    }
    if rule
        .when
        .max_input_tokens
        .is_some_and(|maximum| input_tokens > maximum)
    {
        return false;
    }
    true
}

fn estimate_tokens(prompt: &str) -> u32 {
    let estimate = prompt.chars().count().div_ceil(4);
    u32::try_from(estimate.max(1)).unwrap_or(u32::MAX)
}

fn classification_reasons(classification: Option<&RouteClassification>) -> Vec<String> {
    let Some(classification) = classification else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    if let Some(task) = classification.task {
        parts.push(format!("task={task:?}"));
    }
    if let Some(private) = classification.private {
        parts.push(format!("private={private}"));
    }
    if let Some(long_context) = classification.long_context {
        parts.push(format!("long_context={long_context}"));
    }
    if let Some(repo) = classification.repo {
        parts.push(format!("repo={repo}"));
    }
    if let Some(confidence) = classification.confidence {
        parts.push(format!("confidence={confidence:.2}"));
    }
    let mut reason = if parts.is_empty() {
        "LLM classifier supplied routing signals.".to_string()
    } else {
        format!(
            "LLM classifier supplied routing signals: {}.",
            parts.join(", ")
        )
    };
    if let Some(note) = classification
        .reason
        .as_deref()
        .filter(|note| !note.is_empty())
    {
        reason.push(' ');
        reason.push_str(note);
    }
    vec![reason]
}

fn task_profile(
    prompt: &str,
    input_tokens: u32,
    hint: Option<TaskHint>,
    classification: Option<&RouteClassification>,
) -> TaskProfile {
    let normalized = prompt.to_ascii_lowercase();
    let inferred = hint
        .or_else(|| classification.and_then(|classification| classification.task))
        .unwrap_or_else(|| infer_task_hint(&normalized, input_tokens));
    let codebase_editing = classification
        .and_then(|classification| classification.repo)
        .unwrap_or_else(|| is_codebase_work_request(&normalized));
    let tests = contains_any(&normalized, &["test", "tests", "tdd", "failing"]);
    let privacy_sensitive = classification
        .and_then(|classification| classification.private)
        .unwrap_or_else(|| {
            contains_any(
                &normalized,
                &[
                    "private",
                    "personal",
                    "journal",
                    "confidential",
                    "secret",
                    "keep it local",
                ],
            )
        });
    let long_context = classification
        .and_then(|classification| classification.long_context)
        .unwrap_or_else(|| {
            input_tokens > 32_000
                || contains_any(
                    &normalized,
                    &["long context", "entire repo", "dossier", "large document"],
                )
        });
    let low_risk = inferred == TaskHint::Simple && input_tokens <= 4_000;

    TaskProfile {
        hint: inferred,
        codebase_editing,
        tests,
        privacy_sensitive,
        long_context,
        low_risk,
    }
}

fn infer_task_hint(prompt: &str, input_tokens: u32) -> TaskHint {
    if contains_any(
        prompt,
        &[
            "code",
            "compile",
            "debug",
            "test",
            "rust",
            "typescript",
            "python",
            "cli",
            "refactor",
            "failing",
        ],
    ) || is_codebase_work_request(prompt)
        || is_codebase_reference_request(prompt)
    {
        return TaskHint::Code;
    }
    if input_tokens > 32_000
        || contains_any(
            prompt,
            &["architecture", "legal", "strategy", "reason", "analyze"],
        )
    {
        return TaskHint::DeepReasoning;
    }
    if contains_any(prompt, &["write", "memo", "email", "draft", "essay"]) {
        return TaskHint::Writing;
    }
    TaskHint::Simple
}

fn is_codebase_work_request(prompt: &str) -> bool {
    let has_code_context = contains_any(
        prompt,
        &[
            "repo",
            "repository",
            "codebase",
            "code",
            "cli",
            "rust",
            "typescript",
            "python",
            "file",
            "files",
            "module",
            "function",
            "parser",
            "pull request",
            " pr",
            "test",
            "tests",
        ],
    );
    let has_code_action = contains_any(
        prompt,
        &[
            "edit",
            "fix",
            "failing",
            "refactor",
            "implement",
            "add",
            "update",
            "change",
            "debug",
            "compile",
            "test",
            "tests",
            "review",
        ],
    );

    has_code_context && has_code_action
}

fn is_codebase_reference_request(prompt: &str) -> bool {
    let references_codebase = contains_any(
        prompt,
        &[
            "repo",
            "repository",
            "codebase",
            "this project",
            "project files",
        ],
    );
    let asks_for_understanding = contains_any(
        prompt,
        &[
            "what is",
            "what's",
            "about",
            "summarize",
            "explain",
            "overview",
            "understand",
            "walk me through",
        ],
    );

    references_codebase && asks_for_understanding
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn supports_required_capability(provider: &ProviderConfig, profile: &TaskProfile) -> bool {
    match profile.hint {
        TaskHint::Simple => provider.supports(&Capability::Summarization),
        TaskHint::Code => provider.supports(&Capability::Code),
        TaskHint::DeepReasoning => provider.supports(&Capability::Reasoning),
        TaskHint::Writing => provider.supports(&Capability::Writing),
    }
}

fn score_provider<'a>(
    provider: &'a ProviderConfig,
    profile: &TaskProfile,
    preferred: ProviderId,
    input_tokens: u32,
    output_tokens: u32,
) -> ScoredProvider<'a> {
    let mut score = 0.0;
    let mut reasons = Vec::new();

    match profile.hint {
        TaskHint::Simple => {
            score += 0.35;
            if profile.low_risk && provider.supports(&Capability::Local) {
                score += 0.24;
                reasons.push("Small, low-risk task fits a local model.".to_string());
            }
            if profile.privacy_sensitive && provider.supports(&Capability::Privacy) {
                score += 0.28;
                reasons.push("privacy-sensitive request can stay local.".to_string());
            }
        }
        TaskHint::Code => {
            score += 0.38;
            if profile.codebase_editing && provider.supports(&Capability::CodebaseEditing) {
                score += 0.34;
                reasons.push("codebase editing is a strong fit.".to_string());
            }
            if profile.tests && provider.supports(&Capability::Tests) {
                score += 0.16;
                reasons.push("Test work is a strong fit.".to_string());
            }
            if provider.supports(&Capability::Reasoning) {
                score += 0.06;
            }
        }
        TaskHint::DeepReasoning => {
            score += 0.42;
            if profile.long_context && provider.supports(&Capability::LongContext) {
                score += 0.32;
                reasons.push("Long-context reasoning is a strong fit.".to_string());
            }
        }
        TaskHint::Writing => {
            score += 0.40;
            if provider.supports(&Capability::Reasoning) {
                score += 0.10;
            }
            if profile.long_context && provider.supports(&Capability::LongContext) {
                score += 0.10;
            }
        }
    }

    if provider.id == preferred {
        score += 0.14;
        reasons.push(format!(
            "{} matches the configured routing policy.",
            provider.id.label()
        ));
    }

    let context_headroom = f64::from(provider.max_input_tokens.saturating_sub(input_tokens))
        / f64::from(provider.max_input_tokens.max(1));
    if context_headroom >= 0.25 {
        score += 0.08;
    }

    let estimated_cost = provider.estimated_cost_cents(input_tokens, output_tokens);
    if estimated_cost <= 0.01 {
        score += 0.06;
    }

    ScoredProvider {
        provider,
        score,
        reasons,
    }
}

fn confidence_from_score(score: f64) -> f32 {
    if score >= 1.05 {
        0.92
    } else if score >= 0.80 {
        0.86
    } else if score >= 0.60 {
        0.74
    } else {
        0.62
    }
}
