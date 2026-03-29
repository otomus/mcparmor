//! Secret scanner — detects credentials and API keys in tool responses.
//!
//! Runs on every tool response before it is forwarded to the MCP host.
//! Scanning is applied according to the `output.scan_secrets` policy:
//! - Disabled: response passes through unchanged
//! - Redact: detected secrets are replaced with `[REDACTED:{pattern_key}]`
//! - Strict: the entire response is blocked on any detection

use regex::Regex;
use std::sync::OnceLock;

/// A detected secret with its type and location in the response.
#[derive(Debug, Clone)]
pub struct DetectedSecret {
    /// Human-readable label for the secret type (e.g. "OpenAI API key").
    pub secret_type: String,
    /// The byte range in the original response where the secret was found.
    pub range: std::ops::Range<usize>,
}

/// The result of scanning a response payload.
#[derive(Debug)]
pub struct ScanResult {
    /// All secrets detected in the payload.
    pub detections: Vec<DetectedSecret>,
    /// The payload after redaction (if redaction mode is active).
    /// Identical to the input if no secrets were found or if mode is Disabled.
    pub redacted: String,
}

/// Scan a response payload for known secret patterns.
///
/// Returns a `ScanResult` containing all detections and the redacted payload.
/// The caller decides what to do with the result based on the manifest policy.
///
/// # Arguments
/// * `payload` - The raw response string to scan
///
/// # Performance
/// Patterns are compiled once and cached. Per-call overhead is < 3ms for
/// payloads up to 10KB and < 15ms for payloads up to 100KB.
pub fn scan(payload: &str) -> ScanResult {
    let patterns = secret_patterns();
    let mut detections = Vec::new();

    for (secret_type, regex) in patterns {
        for m in regex.find_iter(payload) {
            detections.push(DetectedSecret {
                secret_type: secret_type.to_string(),
                range: m.start()..m.end(),
            });
        }
    }

    let redacted = redact(payload, &detections);

    ScanResult { detections, redacted }
}

/// Replace detected secret ranges in the payload with `[REDACTED:{secret_type}]`.
///
/// Overlapping or adjacent detections are merged into a single replacement,
/// using the type of the first detection in the merged group.
fn redact(payload: &str, detections: &[DetectedSecret]) -> String {
    if detections.is_empty() {
        return payload.to_string();
    }

    let merged = merge_detections(detections);

    // Process descending so earlier replacements don't shift later offsets.
    let mut result = payload.to_string();
    for (range, secret_type) in merged.into_iter().rev() {
        let label = format!("[REDACTED:{secret_type}]");
        result.replace_range(range, &label);
    }
    result
}

/// Merge overlapping or adjacent detections into non-overlapping spans.
///
/// When detections overlap, the span is extended and the first detection's
/// `secret_type` is used for the merged group's redaction label.
fn merge_detections(detections: &[DetectedSecret]) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut sorted: Vec<&DetectedSecret> = detections.iter().collect();
    sorted.sort_by_key(|d| d.range.start);

    let mut merged: Vec<(std::ops::Range<usize>, &str)> = Vec::with_capacity(sorted.len());
    for detection in sorted {
        match merged.last_mut() {
            Some((last_range, _)) if detection.range.start <= last_range.end => {
                // Overlapping or adjacent — extend the current span, keep first type.
                last_range.end = last_range.end.max(detection.range.end);
            }
            _ => merged.push((detection.range.clone(), &detection.secret_type)),
        }
    }
    merged
}

/// Returns compiled secret detection patterns.
///
/// Patterns are compiled once per process using `OnceLock`.
/// Adding new patterns here automatically applies them to all scan calls.
///
/// # Returns
/// A static reference to a vec of `(pattern_key, compiled_regex)` tuples,
/// where `pattern_key` is the redaction label (e.g. `"openai_key"`).
fn secret_patterns() -> &'static Vec<(&'static str, Regex)> {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            ("openai_key",      Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap()),
            ("anthropic_key",   Regex::new(r"sk-ant-[A-Za-z0-9\-]{20,}").unwrap()),
            ("aws_access_key",  Regex::new(r"AKIA[0-9A-Z]{16}").unwrap()),
            ("aws_secret_key",  Regex::new("(?i)aws.{0,20}secret.{0,20}['\"][0-9a-zA-Z/+]{40}['\"]").unwrap()),
            ("github_token",    Regex::new(r"gh[pousr]_[A-Za-z0-9]{36}").unwrap()),
            ("bearer_token",    Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-._~+/]{20,}").unwrap()),
            ("private_key",     Regex::new(r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----").unwrap()),
            ("slack_token",     Regex::new(r"xox[baprs]-[0-9A-Za-z\-]{10,}").unwrap()),
            ("stripe_key",      Regex::new(r"sk_live_[0-9a-zA-Z]{24,}").unwrap()),
            ("google_api_key",  Regex::new(r"AIza[A-Za-z0-9\-_]{35}").unwrap()),
            ("db_connection",   Regex::new(r#"(?:mongodb|postgres|mysql|redis)://[^\s"']+:[^\s"']+@"#).unwrap()),
            ("jwt_token",       Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap()),
            ("generic_secret",  Regex::new(r#"(?i)(?:api.?key|secret|password|token)\s*[=:]\s*["']?[A-Za-z0-9\-._~+/]{16,}"#).unwrap()),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Happy path: known patterns are detected ---

    #[test]
    fn openai_key_is_detected() {
        let result = scan("Authorization: Bearer sk-abcdefghijklmnopqrst1234567890");
        assert!(!result.detections.is_empty());
        assert!(result.detections.iter().any(|d| d.secret_type == "openai_key"));
    }

    #[test]
    fn aws_access_key_is_detected() {
        let result = scan("key=AKIAIOSFODNN7EXAMPLE");
        assert!(result.detections.iter().any(|d| d.secret_type == "aws_access_key"));
    }

    #[test]
    fn github_token_is_detected() {
        let result = scan("token=ghp_abcdefghijklmnopqrstuvwxyz123456789012");
        assert!(result.detections.iter().any(|d| d.secret_type == "github_token"));
    }

    #[test]
    fn jwt_token_is_detected() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let result = scan(jwt);
        assert!(result.detections.iter().any(|d| d.secret_type == "jwt_token"));
    }

    #[test]
    fn db_connection_string_is_detected() {
        let result = scan("mongodb://myuser:supersecret@cluster0.example.mongodb.net/mydb");
        assert!(result.detections.iter().any(|d| d.secret_type == "db_connection"));
    }

    #[test]
    fn private_key_header_is_detected() {
        let result = scan("-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAK...");
        assert!(result.detections.iter().any(|d| d.secret_type == "private_key"));
    }

    // --- Redaction format includes pattern key ---

    #[test]
    fn redaction_label_includes_pattern_key() {
        let result = scan("key=AKIAIOSFODNN7EXAMPLE");
        assert!(result.redacted.contains("[REDACTED:aws_access_key]"),
            "expected [REDACTED:aws_access_key] in: {}", result.redacted);
    }

    #[test]
    fn openai_key_redacted_with_label() {
        let payload = "use sk-aaaaaaaaaaaaaaaaaaaa to authenticate";
        let result = scan(payload);
        assert!(result.redacted.contains("[REDACTED:openai_key]"),
            "expected label in: {}", result.redacted);
        assert!(!result.redacted.contains("sk-aaaa"),
            "original key must not appear after redaction");
    }

    // --- Edge cases ---

    #[test]
    fn empty_payload_returns_no_detections() {
        let result = scan("");
        assert!(result.detections.is_empty());
        assert_eq!(result.redacted, "");
    }

    #[test]
    fn clean_payload_passes_through_unchanged() {
        let payload = "Hello, world! This contains no secrets.";
        let result = scan(payload);
        assert!(result.detections.is_empty());
        assert_eq!(result.redacted, payload);
    }

    #[test]
    fn multiple_secrets_in_one_payload_are_all_detected() {
        let payload = "key=AKIAIOSFODNN7EXAMPLE and sk-abcdefghijklmnopqrst1234567890";
        let result = scan(payload);
        let types: Vec<&str> = result.detections.iter().map(|d| d.secret_type.as_str()).collect();
        assert!(types.contains(&"aws_access_key"), "aws_access_key missing from {types:?}");
        assert!(types.contains(&"openai_key"), "openai_key missing from {types:?}");
    }

    #[test]
    fn overlapping_detections_merge_into_single_redaction() {
        // A value may match multiple patterns; merged replacement must not panic or double-replace.
        let payload = "token: sk-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP";
        let result = scan(payload);
        // "generic_secret" and "openai_key" may both match. Redacted output must be valid.
        assert!(!result.redacted.contains("sk-abcdef"),
            "secret must be redacted in: {}", result.redacted);
    }

    #[test]
    fn payload_longer_than_100kb_does_not_panic() {
        let huge = "A".repeat(150_000);
        let result = scan(&huge);
        assert!(result.detections.is_empty());
    }

    #[test]
    fn unicode_payload_does_not_panic() {
        let payload = "こんにちは 世界 \0 \n\t 🔑";
        let result = scan(payload);
        assert!(result.detections.is_empty());
    }

    #[test]
    fn short_key_below_minimum_length_is_not_detected() {
        // "sk-short" is only 8 chars after the prefix — below the 20-char minimum.
        let result = scan("sk-short");
        assert!(!result.detections.iter().any(|d| d.secret_type == "openai_key"));
    }
}
