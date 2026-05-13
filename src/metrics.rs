use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::RequestLogEntry;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouterMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub estimated_cost_cents: f64,
    pub by_provider: Vec<ProviderMetrics>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderMetrics {
    pub provider: String,
    pub requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub estimated_cost_cents: f64,
}

#[derive(Default)]
struct ProviderAccumulator {
    requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    estimated_cost_cents: f64,
}

pub fn metrics_from_logs(entries: &[RequestLogEntry]) -> RouterMetrics {
    let mut by_provider = BTreeMap::<String, ProviderAccumulator>::new();
    let mut metrics = RouterMetrics {
        total_requests: 0,
        successful_requests: 0,
        failed_requests: 0,
        estimated_cost_cents: 0.0,
        by_provider: Vec::new(),
    };

    for entry in entries {
        metrics.total_requests += 1;
        let cost_cents = entry
            .actual_cost_cents
            .unwrap_or(entry.estimated_cost_cents);
        metrics.estimated_cost_cents += cost_cents;
        if entry.success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        let provider = by_provider.entry(entry.provider.clone()).or_default();
        provider.requests += 1;
        provider.estimated_cost_cents += cost_cents;
        if entry.success {
            provider.successful_requests += 1;
        } else {
            provider.failed_requests += 1;
        }
    }

    metrics.by_provider = by_provider
        .into_iter()
        .map(|(provider, values)| ProviderMetrics {
            provider,
            requests: values.requests,
            successful_requests: values.successful_requests,
            failed_requests: values.failed_requests,
            estimated_cost_cents: values.estimated_cost_cents,
        })
        .collect();
    metrics
}

pub fn load_metrics(path: &Path) -> std::io::Result<RouterMetrics> {
    let file = File::open(path)?;
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry =
            serde_json::from_str::<RequestLogEntry>(&line).map_err(std::io::Error::other)?;
        entries.push(entry);
    }
    Ok(metrics_from_logs(&entries))
}
