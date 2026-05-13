use std::fmt;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::RequestLogEntry;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendProvider {
    OpenAi,
    Anthropic,
    Google,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SecretRef {
    Env { name: String },
}

impl SecretRef {
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Env { name } => std::env::var(name).ok(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpendWindow {
    pub start_unix_seconds: u64,
    pub end_unix_seconds: u64,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
pub struct SpendHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl fmt::Debug for SpendHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SpendHttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &redacted_headers(&self.headers))
            .finish()
    }
}

impl Serialize for SpendHttpRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("SpendHttpRequest", 3)?;
        state.serialize_field("method", &self.method)?;
        state.serialize_field("url", &self.url)?;
        state.serialize_field("headers", &redacted_headers(&self.headers))?;
        state.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderSpendReport {
    pub provider: SpendProvider,
    pub total_cents: f64,
    pub currency: String,
    pub items: Vec<SpendItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpendItem {
    pub label: String,
    pub cents: f64,
    pub currency: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub monthly_api_budget_cents: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetState {
    pub actual_monthly_api_spend_cents: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetDecision {
    pub allowed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpendReconciliation {
    pub estimated_total_cents: f64,
    pub actual_total_cents: f64,
    pub delta_cents: f64,
}

#[derive(Debug, Error)]
pub enum SpendSyncError {
    #[error("spend request to {url} failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("spend endpoint returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("spend response parse failed: {source}")]
    Parse { source: serde_json::Error },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SpendInputError {
    #[error("invalid Google billing table identifier: {value}")]
    InvalidGoogleBillingTable { value: String },
    #[error("invalid Google billing date: {value}")]
    InvalidGoogleBillingDate { value: String },
}

pub fn build_openai_cost_request(window: SpendWindow, api_key: &str) -> SpendHttpRequest {
    SpendHttpRequest {
        method: "GET".to_string(),
        url: format!(
            "https://api.openai.com/v1/organization/costs?start_time={}&end_time={}&bucket_width=1d",
            window.start_unix_seconds, window.end_unix_seconds
        ),
        headers: vec![("Authorization".to_string(), format!("Bearer {api_key}"))],
    }
}

pub fn sync_openai_costs(
    window: SpendWindow,
    api_key: &str,
) -> Result<ProviderSpendReport, SpendSyncError> {
    let request = build_openai_cost_request(window, api_key);
    let body = execute_spend_request(&request)?;
    parse_openai_costs_response(&body).map_err(|source| SpendSyncError::Parse { source })
}

pub fn build_anthropic_cost_request(
    starting_at: &str,
    ending_at: &str,
    admin_api_key: &str,
) -> SpendHttpRequest {
    SpendHttpRequest {
        method: "GET".to_string(),
        url: format!(
            "https://api.anthropic.com/v1/organizations/cost_report?starting_at={}&ending_at={}&bucket_width=1d",
            url_encode(starting_at),
            url_encode(ending_at)
        ),
        headers: vec![
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
            ("x-api-key".to_string(), admin_api_key.to_string()),
        ],
    }
}

pub fn sync_anthropic_cost_report(
    starting_at: &str,
    ending_at: &str,
    admin_api_key: &str,
) -> Result<ProviderSpendReport, SpendSyncError> {
    let request = build_anthropic_cost_request(starting_at, ending_at, admin_api_key);
    let body = execute_spend_request(&request)?;
    parse_anthropic_cost_report(&body).map_err(|source| SpendSyncError::Parse { source })
}

pub fn try_build_google_billing_query(
    table: &str,
    start_date: &str,
    end_date: &str,
) -> Result<String, SpendInputError> {
    let table = validate_google_billing_table(table)?;
    let start_date = validate_google_billing_date(start_date)?;
    let end_date = validate_google_billing_date(end_date)?;
    Ok(format!(
        "SELECT service.description, SUM(cost) AS cost, currency \
         FROM {table} \
         WHERE usage_start_time >= TIMESTAMP('{start_date}') \
           AND usage_start_time < TIMESTAMP('{end_date}') \
           AND (service.description LIKE '%Gemini%' OR sku.description LIKE '%Gemini%' OR service.description LIKE '%Vertex AI%') \
         GROUP BY service.description, currency \
         ORDER BY cost DESC"
    ))
}

pub fn build_google_billing_query(table: &str, start_date: &str, end_date: &str) -> String {
    try_build_google_billing_query(table, start_date, end_date)
        .expect("Google billing query inputs must be validated")
}

pub fn parse_openai_costs_response(body: &str) -> Result<ProviderSpendReport, serde_json::Error> {
    let value = serde_json::from_str::<Value>(body)?;
    let mut items = Vec::new();
    for result in nested_results(&value) {
        let dollars = result
            .pointer("/amount/value")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let currency = result
            .pointer("/amount/currency")
            .and_then(Value::as_str)
            .unwrap_or("usd")
            .to_ascii_lowercase();
        let label = result
            .get("line_item")
            .and_then(Value::as_str)
            .or_else(|| result.get("project_id").and_then(Value::as_str))
            .unwrap_or("cost")
            .to_string();
        items.push(SpendItem {
            label,
            cents: dollars * 100.0,
            currency,
        });
    }
    Ok(report(SpendProvider::OpenAi, items, "usd"))
}

pub fn parse_anthropic_cost_report(body: &str) -> Result<ProviderSpendReport, serde_json::Error> {
    let value = serde_json::from_str::<Value>(body)?;
    let mut items = Vec::new();
    for result in nested_results(&value) {
        let cents = result
            .get("amount")
            .and_then(parse_string_or_number)
            .or_else(|| result.get("cost").and_then(parse_string_or_number))
            .or_else(|| {
                result
                    .pointer("/amount/value")
                    .and_then(parse_string_or_number)
            })
            .unwrap_or_default();
        let currency = result
            .get("currency")
            .and_then(Value::as_str)
            .or_else(|| result.pointer("/amount/currency").and_then(Value::as_str))
            .unwrap_or("USD")
            .to_ascii_lowercase();
        let label = result
            .get("description")
            .and_then(Value::as_str)
            .or_else(|| result.get("workspace_id").and_then(Value::as_str))
            .unwrap_or("cost")
            .to_string();
        items.push(SpendItem {
            label,
            cents,
            currency,
        });
    }
    Ok(report(SpendProvider::Anthropic, items, "usd"))
}

pub fn parse_google_billing_rows(body: &str) -> Result<ProviderSpendReport, serde_json::Error> {
    let value = serde_json::from_str::<Value>(body)?;
    let mut items = Vec::new();
    if let Some(rows) = value.get("rows").and_then(Value::as_array) {
        for row in rows {
            let fields = row
                .get("f")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let label = field_value(fields, 0).unwrap_or("Gemini").to_string();
            let dollars = field_value(fields, 1)
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or_default();
            let currency = field_value(fields, 2).unwrap_or("USD").to_ascii_lowercase();
            items.push(SpendItem {
                label,
                cents: dollars * 100.0,
                currency,
            });
        }
    }
    Ok(report(SpendProvider::Google, items, "usd"))
}

pub fn check_api_budget(
    config: &BudgetConfig,
    state: &BudgetState,
    projected_request_cents: f64,
) -> BudgetDecision {
    let Some(limit) = config.monthly_api_budget_cents else {
        return BudgetDecision {
            allowed: true,
            reason: "No monthly API budget configured.".to_string(),
        };
    };
    let projected_total = state.actual_monthly_api_spend_cents + projected_request_cents;
    if projected_total <= limit {
        return BudgetDecision {
            allowed: true,
            reason: format!("Projected API spend {projected_total:.2} cents is within budget."),
        };
    }
    BudgetDecision {
        allowed: false,
        reason: format!(
            "Projected API spend {projected_total:.2} cents exceeds monthly API budget {limit:.2} cents."
        ),
    }
}

pub fn reconcile_spend(
    logs: &[RequestLogEntry],
    reports: &[ProviderSpendReport],
) -> SpendReconciliation {
    let estimated_total_cents = logs
        .iter()
        .map(|entry| {
            entry
                .actual_cost_cents
                .unwrap_or(entry.estimated_cost_cents)
        })
        .sum::<f64>();
    let actual_total_cents = reports.iter().map(|report| report.total_cents).sum::<f64>();
    SpendReconciliation {
        estimated_total_cents,
        actual_total_cents,
        delta_cents: actual_total_cents - estimated_total_cents,
    }
}

pub fn execute_spend_request(request: &SpendHttpRequest) -> Result<String, SpendSyncError> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| SpendSyncError::Request {
            url: request.url.clone(),
            source,
        })?;
    let method =
        reqwest::Method::from_bytes(request.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    let response = builder.send().map_err(|source| SpendSyncError::Request {
        url: request.url.clone(),
        source,
    })?;
    let status = response.status();
    let body = response.text().map_err(|source| SpendSyncError::Request {
        url: request.url.clone(),
        source,
    })?;
    if !status.is_success() {
        return Err(SpendSyncError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    Ok(body)
}

fn nested_results(value: &Value) -> Vec<&Value> {
    let mut results = Vec::new();
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        for bucket in data {
            if let Some(bucket_results) = bucket.get("results").and_then(Value::as_array) {
                results.extend(bucket_results);
            } else {
                results.push(bucket);
            }
        }
    }
    if results.is_empty()
        && let Some(values) = value.get("results").and_then(Value::as_array)
    {
        results.extend(values);
    }
    results
}

fn report(
    provider: SpendProvider,
    items: Vec<SpendItem>,
    default_currency: &str,
) -> ProviderSpendReport {
    let total_cents = items.iter().map(|item| item.cents).sum();
    let currency = items
        .first()
        .map(|item| item.currency.clone())
        .unwrap_or_else(|| default_currency.to_string());
    ProviderSpendReport {
        provider,
        total_cents,
        currency,
        items,
    }
}

fn parse_string_or_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
}

fn field_value(fields: &[Value], index: usize) -> Option<&str> {
    fields
        .get(index)
        .and_then(|field| field.get("v"))
        .and_then(Value::as_str)
}

fn redacted_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let value = if sensitive_header(name) {
                "[REDACTED]".to_string()
            } else {
                value.clone()
            };
            (name.clone(), value)
        })
        .collect()
}

fn sensitive_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "authorization" | "proxy-authorization" | "x-api-key"
    ) || normalized.contains("api-key")
}

fn validate_google_billing_table(table: &str) -> Result<&str, SpendInputError> {
    let table = table.trim();
    let identifier = match table.strip_prefix('`') {
        Some(without_prefix) => without_prefix.strip_suffix('`').ok_or_else(|| {
            SpendInputError::InvalidGoogleBillingTable {
                value: table.to_string(),
            }
        })?,
        None if table.contains('`') => {
            return Err(SpendInputError::InvalidGoogleBillingTable {
                value: table.to_string(),
            });
        }
        None => table,
    };

    if identifier.is_empty()
        || !identifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        return Err(SpendInputError::InvalidGoogleBillingTable {
            value: table.to_string(),
        });
    }

    Ok(table)
}

fn validate_google_billing_date(date: &str) -> Result<&str, SpendInputError> {
    let bytes = date.as_bytes();
    let valid_shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !valid_shape {
        return Err(SpendInputError::InvalidGoogleBillingDate {
            value: date.to_string(),
        });
    }

    let month = date[5..7].parse::<u32>().unwrap_or_default();
    let day = date[8..10].parse::<u32>().unwrap_or_default();
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(SpendInputError::InvalidGoogleBillingDate {
            value: date.to_string(),
        });
    }

    Ok(date)
}

fn url_encode(value: &str) -> String {
    value
        .replace(':', "%3A")
        .replace('/', "%2F")
        .replace('+', "%2B")
}
