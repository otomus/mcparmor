//! Audit log — structured logging of all broker decisions.
//!
//! Every tool invocation, policy violation, and secret detection is recorded.
//! The audit log is the source of truth for `mcparmor audit` queries.
//! Log entries are written as newline-delimited JSON to a rotating log file.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// A single audit log entry.
#[derive(Debug, Serialize)]
pub struct AuditEntry {
    /// ISO8601 timestamp of the event.
    pub timestamp: DateTime<Utc>,
    /// Name of the tool that triggered the event.
    pub tool: String,
    /// The type of event recorded.
    pub event: AuditEvent,
    /// Additional context for the event (optional).
    pub detail: Option<String>,
}

/// The category of event recorded in the audit log.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    /// A tool call was received and forwarded to the tool subprocess.
    Invoke,
    /// A tool response was received and forwarded to the MCP host.
    Response,
    /// A parameter value violated the manifest policy.
    ParamViolation,
    /// A secret pattern was detected in a tool response.
    SecretDetected,
    /// The tool exceeded its declared timeout.
    Timeout,
    /// The OS sandbox blocked a syscall (logged at Layer 2).
    SandboxViolation,
}

impl AuditEntry {
    /// Create a new invoke entry for a tool call.
    pub fn invoke(tool: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::Invoke,
            detail: Some(method.into()),
        }
    }

    /// Create a new response entry with size and latency information.
    pub fn response(tool: impl Into<String>, size_bytes: usize, latency_ms: u64) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::Response,
            detail: Some(format!("{}B {}ms", size_bytes, latency_ms)),
        }
    }

    /// Create a new param violation entry.
    pub fn param_violation(tool: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::ParamViolation,
            detail: Some(detail.into()),
        }
    }

    /// Create a new secret detected entry.
    pub fn secret_detected(tool: impl Into<String>, secret_type: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::SecretDetected,
            detail: Some(secret_type.into()),
        }
    }

    /// Create a new timeout entry for a tool that exceeded its deadline.
    pub fn timeout(tool: impl Into<String>, timeout_ms: u32) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::Timeout,
            detail: Some(format!("exceeded {}ms", timeout_ms)),
        }
    }

    /// Create a new sandbox violation entry for a blocked OS-level syscall.
    pub fn sandbox_violation(tool: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            event: AuditEvent::SandboxViolation,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // AuditEntry factory methods
    // ---------------------------------------------------------------------------

    #[test]
    fn invoke_sets_correct_fields() {
        let entry = AuditEntry::invoke("my-tool", "tools/call");
        assert_eq!(entry.tool, "my-tool");
        assert!(matches!(entry.event, AuditEvent::Invoke));
        assert_eq!(entry.detail.as_deref(), Some("tools/call"));
    }

    #[test]
    fn response_sets_correct_fields() {
        let entry = AuditEntry::response("my-tool", 1024, 50);
        assert_eq!(entry.tool, "my-tool");
        assert!(matches!(entry.event, AuditEvent::Response));
        // Detail must encode both size and latency.
        let detail = entry.detail.unwrap();
        assert!(detail.contains("1024B"), "detail missing size: {detail}");
        assert!(detail.contains("50ms"), "detail missing latency: {detail}");
    }

    #[test]
    fn param_violation_sets_correct_fields() {
        let entry = AuditEntry::param_violation("tool-a", "path traversal attempt");
        assert_eq!(entry.tool, "tool-a");
        assert!(matches!(entry.event, AuditEvent::ParamViolation));
        assert_eq!(entry.detail.as_deref(), Some("path traversal attempt"));
    }

    #[test]
    fn secret_detected_sets_correct_fields() {
        let entry = AuditEntry::secret_detected("tool-a", "openai_key");
        assert_eq!(entry.tool, "tool-a");
        assert!(matches!(entry.event, AuditEvent::SecretDetected));
        assert_eq!(entry.detail.as_deref(), Some("openai_key"));
    }

    #[test]
    fn timeout_sets_correct_fields() {
        let entry = AuditEntry::timeout("slow-tool", 30000);
        assert_eq!(entry.tool, "slow-tool");
        assert!(matches!(entry.event, AuditEvent::Timeout));
        let detail = entry.detail.unwrap();
        assert!(detail.contains("30000ms"), "detail missing timeout: {detail}");
    }

    #[test]
    fn sandbox_violation_sets_correct_fields() {
        let entry = AuditEntry::sandbox_violation("bad-tool", "execve blocked");
        assert_eq!(entry.tool, "bad-tool");
        assert!(matches!(entry.event, AuditEvent::SandboxViolation));
        assert_eq!(entry.detail.as_deref(), Some("execve blocked"));
    }

    // ---------------------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------------------

    #[test]
    fn invoke_with_empty_tool_name_does_not_panic() {
        let entry = AuditEntry::invoke("", "tools/call");
        assert_eq!(entry.tool, "");
    }

    #[test]
    fn invoke_with_empty_method_does_not_panic() {
        let entry = AuditEntry::invoke("tool", "");
        assert_eq!(entry.detail.as_deref(), Some(""));
    }

    #[test]
    fn response_with_zero_size_and_zero_latency() {
        let entry = AuditEntry::response("tool", 0, 0);
        let detail = entry.detail.unwrap();
        assert!(detail.contains("0B"), "detail missing size: {detail}");
        assert!(detail.contains("0ms"), "detail missing latency: {detail}");
    }

    #[test]
    fn audit_event_serializes_to_snake_case() {
        // serde(rename_all = "snake_case") must apply to all variants.
        let entry = AuditEntry::secret_detected("tool", "key_type");
        let json = serde_json::to_string(&entry).unwrap();
        assert!(
            json.contains("\"secret_detected\""),
            "event must serialise as snake_case; got: {json}"
        );
    }

    #[test]
    fn audit_entry_timestamp_is_in_utc() {
        let before = Utc::now();
        let entry = AuditEntry::invoke("tool", "method");
        let after = Utc::now();
        assert!(
            entry.timestamp >= before && entry.timestamp <= after,
            "timestamp must be set to current UTC time"
        );
    }
}
