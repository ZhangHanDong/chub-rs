//! Analytics — fire-and-forget event tracking.
//!
//! Non-blocking: never throws, never blocks the main command path.

/// Track an analytics event. No-op if telemetry is disabled.
pub fn track_event(_event: &str, _properties: &serde_json::Value, _telemetry_enabled: bool) {
    // Fire-and-forget: in a real implementation this would POST to PostHog.
    // For now, this is a no-op placeholder that satisfies the non-blocking contract.
}

/// Shutdown analytics. Always succeeds.
pub fn shutdown_analytics() {
    // No-op: nothing to flush in the current implementation.
}
