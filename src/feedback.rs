use std::{fs::OpenOptions, io::Write, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub route_id: String,
    pub provider: String,
    pub rating: String,
    pub note: Option<String>,
}

pub fn append_feedback(path: &Path, entry: &FeedbackEntry) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")?;
    Ok(())
}
