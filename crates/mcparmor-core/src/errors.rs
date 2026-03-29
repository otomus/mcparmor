//! Structured error codes returned to the MCP host on policy violations.
//!
//! Error codes are in the JSON-RPC application error range (-32099 to -32000).
//! Every error includes a machine-readable `code`, a human-readable `message`,
//! and an optional `hint` explaining what the tool author can do to fix it.

use serde::Serialize;

/// A JSON-RPC error response body for broker policy violations.
#[derive(Debug, Serialize)]
pub struct BrokerError {
    /// Machine-readable error code in the -32001 to -32006 range.
    pub code: i32,
    /// Human-readable description of what was blocked.
    pub message: String,
    /// Actionable hint for the tool author or operator.
    pub hint: Option<String>,
}

/// All broker error codes with their semantics.
pub mod codes {
    /// Path violation: the tool attempted to access a filesystem path
    /// outside its declared `filesystem.read` or `filesystem.write` allowlist.
    pub const PATH_VIOLATION: i32 = -32001;

    /// Network violation: the tool attempted to connect to a host or port
    /// not in its `network.allow` list.
    pub const NETWORK_VIOLATION: i32 = -32002;

    /// Spawn violation: the tool attempted to spawn a child process
    /// but `spawn: false` is declared in the manifest.
    pub const SPAWN_VIOLATION: i32 = -32003;

    /// Secret detected: the tool's response contains a pattern matching
    /// a known secret format (API key, token, credential).
    pub const SECRET_DETECTED: i32 = -32004;

    /// Timeout: the tool did not respond within `timeout_ms` milliseconds.
    pub const TIMEOUT: i32 = -32005;

    /// Manifest error: the armor.json is invalid, missing, or incompatible
    /// with the current broker spec version.
    pub const MANIFEST_ERROR: i32 = -32006;
}

impl BrokerError {
    /// Construct a path violation error with the attempted path as context.
    pub fn path_violation(attempted_path: &str) -> Self {
        Self {
            code: codes::PATH_VIOLATION,
            message: format!("Path access denied: '{attempted_path}' is outside the declared filesystem allowlist"),
            hint: Some("Update the filesystem.read or filesystem.write field in armor.json to include this path, then revalidate with `mcparmor validate`.".to_string()),
        }
    }

    /// Construct a network violation error with the attempted destination.
    pub fn network_violation(host: &str, port: u16) -> Self {
        Self {
            code: codes::NETWORK_VIOLATION,
            message: format!("Network access denied: '{host}:{port}' is not in the declared network.allow list"),
            hint: Some(format!("Add \"{host}:{port}\" to network.allow in armor.json, then revalidate with `mcparmor validate`.")),
        }
    }

    /// Construct a spawn violation error.
    pub fn spawn_violation(attempted_command: &str) -> Self {
        Self {
            code: codes::SPAWN_VIOLATION,
            message: format!("Spawn blocked: tool attempted to execute '{attempted_command}'"),
            hint: Some("If this tool requires spawning child processes, set spawn: true in armor.json and document the reason in your PR.".to_string()),
        }
    }

    /// Construct a secret detected error.
    pub fn secret_detected(secret_type: &str) -> Self {
        Self {
            code: codes::SECRET_DETECTED,
            message: format!("Response blocked: detected {secret_type} in tool output"),
            hint: Some("The tool returned what appears to be a secret. If this is a false positive, adjust the scan_secrets setting in armor.json.".to_string()),
        }
    }

    /// Construct a timeout error.
    pub fn timeout(timeout_ms: u32) -> Self {
        Self {
            code: codes::TIMEOUT,
            message: format!("Tool timed out after {timeout_ms}ms"),
            hint: Some("Increase timeout_ms in armor.json if this tool requires more time, or investigate why the tool is not responding.".to_string()),
        }
    }

    /// Construct a manifest error with the validation message.
    pub fn manifest_error(reason: &str) -> Self {
        Self {
            code: codes::MANIFEST_ERROR,
            message: format!("Invalid armor.json: {reason}"),
            hint: Some("Run `mcparmor validate` for detailed schema errors and fix suggestions.".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // BrokerError factory methods — correct code + non-empty message
    // ---------------------------------------------------------------------------

    #[test]
    fn path_violation_has_correct_code_and_embeds_path() {
        let err = BrokerError::path_violation("/etc/passwd");
        assert_eq!(err.code, codes::PATH_VIOLATION);
        assert!(
            err.message.contains("/etc/passwd"),
            "message must embed the attempted path: {}", err.message
        );
        assert!(err.hint.is_some());
    }

    #[test]
    fn network_violation_has_correct_code_and_embeds_host_and_port() {
        let err = BrokerError::network_violation("evil.com", 443);
        assert_eq!(err.code, codes::NETWORK_VIOLATION);
        assert!(err.message.contains("evil.com"), "message must embed host: {}", err.message);
        assert!(err.message.contains("443"), "message must embed port: {}", err.message);
        assert!(err.hint.is_some());
    }

    #[test]
    fn spawn_violation_has_correct_code_and_embeds_command() {
        let err = BrokerError::spawn_violation("curl");
        assert_eq!(err.code, codes::SPAWN_VIOLATION);
        assert!(err.message.contains("curl"), "message must embed command: {}", err.message);
        assert!(err.hint.is_some());
    }

    #[test]
    fn secret_detected_has_correct_code_and_embeds_secret_type() {
        let err = BrokerError::secret_detected("openai_key");
        assert_eq!(err.code, codes::SECRET_DETECTED);
        assert!(
            err.message.contains("openai_key"),
            "message must embed secret type: {}", err.message
        );
        assert!(err.hint.is_some());
    }

    #[test]
    fn timeout_has_correct_code_and_embeds_duration() {
        let err = BrokerError::timeout(30000);
        assert_eq!(err.code, codes::TIMEOUT);
        assert!(err.message.contains("30000"), "message must embed timeout_ms: {}", err.message);
        assert!(err.hint.is_some());
    }

    #[test]
    fn manifest_error_has_correct_code_and_embeds_reason() {
        let err = BrokerError::manifest_error("missing required field 'version'");
        assert_eq!(err.code, codes::MANIFEST_ERROR);
        assert!(
            err.message.contains("missing required field 'version'"),
            "message must embed reason: {}", err.message
        );
        assert!(err.hint.is_some());
    }

    // ---------------------------------------------------------------------------
    // Error codes are distinct (guards against copy-paste errors)
    // ---------------------------------------------------------------------------

    #[test]
    fn error_codes_are_all_distinct() {
        let codes_list = [
            codes::PATH_VIOLATION,
            codes::NETWORK_VIOLATION,
            codes::SPAWN_VIOLATION,
            codes::SECRET_DETECTED,
            codes::TIMEOUT,
            codes::MANIFEST_ERROR,
        ];
        let mut seen = std::collections::HashSet::new();
        for code in codes_list {
            assert!(seen.insert(code), "duplicate error code: {code}");
        }
    }

    // ---------------------------------------------------------------------------
    // Error codes are in the JSON-RPC application error range
    // ---------------------------------------------------------------------------

    #[test]
    fn all_error_codes_are_in_application_error_range() {
        // JSON-RPC application error range: -32099 to -32000.
        let codes_list = [
            codes::PATH_VIOLATION,
            codes::NETWORK_VIOLATION,
            codes::SPAWN_VIOLATION,
            codes::SECRET_DETECTED,
            codes::TIMEOUT,
            codes::MANIFEST_ERROR,
        ];
        for code in codes_list {
            assert!(
                (-32099..=-32000).contains(&code),
                "code {code} is outside the JSON-RPC application error range -32099..-32000"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------------------

    #[test]
    fn path_violation_with_empty_path_does_not_panic() {
        let err = BrokerError::path_violation("");
        assert_eq!(err.code, codes::PATH_VIOLATION);
    }

    #[test]
    fn network_violation_with_zero_port_does_not_panic() {
        let err = BrokerError::network_violation("host", 0);
        assert_eq!(err.code, codes::NETWORK_VIOLATION);
    }

    #[test]
    fn manifest_error_with_empty_reason_does_not_panic() {
        let err = BrokerError::manifest_error("");
        assert_eq!(err.code, codes::MANIFEST_ERROR);
    }
}
