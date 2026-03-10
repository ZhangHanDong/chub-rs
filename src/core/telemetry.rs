//! Feedback telemetry — HTTP POST with 3s timeout.
//!
//! Compatible with JS `lib/telemetry.js`.

use anyhow::Result;
use serde::Serialize;
use std::time::Duration;

const FEEDBACK_TIMEOUT: Duration = Duration::from_secs(3);

/// Valid feedback labels.
pub const VALID_LABELS: &[&str] = &[
    "outdated",
    "inaccurate",
    "incomplete",
    "well-written",
    "comprehensive",
    "good-examples",
];

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackPayload {
    pub entry_id: String,
    pub entry_type: String,
    pub rating: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub client_id: String,
}

#[derive(Debug, Clone)]
pub enum FeedbackResult {
    Sent,
    Skipped,
}

/// Send feedback to the telemetry endpoint.
/// Returns `Skipped` if telemetry is disabled.
pub fn send_feedback(
    payload: &FeedbackPayload,
    endpoint: &str,
    telemetry_enabled: bool,
) -> Result<FeedbackResult> {
    if !telemetry_enabled {
        return Ok(FeedbackResult::Skipped);
    }

    send_feedback_http(payload, endpoint)
}

/// Internal HTTP POST (separated for testability).
fn send_feedback_http(payload: &FeedbackPayload, endpoint: &str) -> Result<FeedbackResult> {
    let url = format!("{}/feedback", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(FEEDBACK_TIMEOUT)
        .build()?;

    let _response = client.post(&url).json(payload).send()?;

    Ok(FeedbackResult::Sent)
}

/// Send feedback using an injected HTTP function (for testing).
pub fn send_feedback_with<F>(
    payload: &FeedbackPayload,
    telemetry_enabled: bool,
    sender: F,
) -> Result<FeedbackResult>
where
    F: FnOnce(&FeedbackPayload) -> Result<()>,
{
    if !telemetry_enabled {
        return Ok(FeedbackResult::Skipped);
    }
    sender(payload)?;
    Ok(FeedbackResult::Sent)
}
