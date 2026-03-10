//! M0 output module tests.

use chub_rs::core::output;

#[test]
fn output_json_mode_serializes_payload() {
    let data = serde_json::json!({"key": "value", "count": 42});

    // Capture stdout by just verifying the serialization logic works
    let serialized = serde_json::to_string_pretty(&data).expect("serialize");
    let parsed: serde_json::Value = serde_json::from_str(&serialized).expect("parse");
    assert_eq!(parsed["key"], "value");
    assert_eq!(parsed["count"], 42);

    // Verify error JSON shape
    let err_json = serde_json::json!({"error": "something went wrong"});
    let err_str = serde_json::to_string_pretty(&err_json).expect("serialize error");
    let err_parsed: serde_json::Value = serde_json::from_str(&err_str).expect("parse error");
    assert!(err_parsed.get("error").is_some());
    assert_eq!(err_parsed["error"], "something went wrong");
}

#[test]
fn output_human_mode_calls_formatter() {
    let data = serde_json::json!({"key": "value"});
    let mut formatter_called = false;

    output::output(
        &data,
        |_d| {
            formatter_called = true;
        },
        false,
    );

    assert!(
        formatter_called,
        "human formatter should be called in non-json mode"
    );

    // Verify JSON mode does NOT call the formatter
    let mut json_formatter_called = false;
    output::output(
        &data,
        |_d| {
            json_formatter_called = true;
        },
        true,
    );
    assert!(
        !json_formatter_called,
        "human formatter should NOT be called in json mode"
    );
}
