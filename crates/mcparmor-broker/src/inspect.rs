//! Layer 1 parameter inspection — protocol-level enforcement.
//!
//! Validates all JSON-RPC `tools/call` parameters against the manifest policy
//! before forwarding to the tool subprocess. This is the first enforcement layer
//! and runs on every platform regardless of OS sandbox availability.
//!
//! Non-`tools/call` messages are passed through unchanged.

use mcparmor_core::errors::BrokerError;
use mcparmor_core::manifest::ArmorManifest;
use mcparmor_core::policy;
use serde_json::Value;

/// The outcome of inspecting a single JSON-RPC message.
#[derive(Debug)]
pub enum InspectResult {
    /// The message is allowed to pass through to the tool.
    Allow,
    /// The message violates the manifest policy. Contains the structured error.
    Deny(BrokerError),
}

/// Inspect a JSON-RPC message against the manifest policy.
///
/// Only `tools/call` requests are inspected. All other methods are passed
/// through unchanged. Inspection checks string-typed parameter values for:
/// - Path traversal sequences (`..`)
/// - Absolute filesystem paths not covered by the declared allowlist
/// - URLs targeting hosts not in `network.allow`
///
/// # Arguments
/// * `message` - The parsed JSON-RPC message received from the host
/// * `manifest` - The armor manifest declaring the tool's capability policy
pub fn check_message(message: &Value, manifest: &ArmorManifest) -> InspectResult {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    if method != "tools/call" {
        return InspectResult::Allow;
    }

    let arguments = message
        .get("params")
        .and_then(|p| p.get("arguments"))
        .unwrap_or(&Value::Null);

    let strings = extract_strings(arguments);
    for s in &strings {
        match check_string(s, manifest) {
            InspectResult::Deny(err) => return InspectResult::Deny(err),
            InspectResult::Allow => {}
        }
    }

    InspectResult::Allow
}

/// Check a single string value extracted from tool parameters.
fn check_string(s: &str, manifest: &ArmorManifest) -> InspectResult {
    if contains_path_traversal(s) {
        return InspectResult::Deny(BrokerError::path_violation(s));
    }
    if looks_like_path(s) {
        return check_path(s, manifest);
    }
    if looks_like_url(s) {
        return check_url(s, manifest);
    }
    InspectResult::Allow
}

/// Returns true if the string contains a `..` path traversal sequence.
///
/// Checks for `..` as a complete path component (separated by `/` or `\`).
/// This avoids false positives on strings like `release..notes.md` or
/// `abc..def` that contain `..` but are not traversal attempts.
///
/// Decodes the three percent-encoded characters that can encode traversal
/// sequences before checking: `%2e` → `.`, `%2f` → `/`, `%5c` → `\`.
fn contains_path_traversal(s: &str) -> bool {
    let decoded = decode_traversal_chars(s);
    decoded.split(['/', '\\']).any(|component| component == "..")
}

/// Decode percent-encoded `.` (`%2e`), `/` (`%2f`), and `\` (`%5c`) in `s`.
///
/// Only these three encodings are relevant to path traversal. All other
/// percent-encoded sequences are left unchanged.
fn decode_traversal_chars(s: &str) -> String {
    // Fast path: no percent sign means nothing to decode.
    if !s.contains('%') {
        return s.to_owned();
    }
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        let a = chars.next();
        let b = chars.next();
        match (a, b) {
            (Some(a), Some(b)) => {
                match (a.to_ascii_lowercase(), b.to_ascii_lowercase()) {
                    ('2', 'e') => result.push('.'),
                    ('2', 'f') => result.push('/'),
                    ('5', 'c') => result.push('\\'),
                    _ => { result.push('%'); result.push(a); result.push(b); }
                }
            }
            (Some(a), None) => { result.push('%'); result.push(a); }
            _ => result.push('%'),
        }
    }
    result
}

/// Returns true if the string looks like an absolute filesystem path.
///
/// Matches strings starting with `/` or `~/`.
fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with("~/")
}

/// Returns true if the string looks like a URL (contains `://`).
fn looks_like_url(s: &str) -> bool {
    s.contains("://")
}

/// Check an absolute path string against the manifest filesystem allowlist.
///
/// A path is allowed if it matches any entry in `filesystem.read` or
/// `filesystem.write`. If both lists are empty, all paths are denied.
fn check_path(path: &str, manifest: &ArmorManifest) -> InspectResult {
    if policy::allows_path_read(manifest, path) || policy::allows_path_write(manifest, path) {
        InspectResult::Allow
    } else {
        InspectResult::Deny(BrokerError::path_violation(path))
    }
}

/// Check a URL string against the manifest network allowlist.
fn check_url(url: &str, manifest: &ArmorManifest) -> InspectResult {
    let Some((host, port)) = parse_host_port(url) else {
        // Cannot parse host/port — deny by default.
        return InspectResult::Deny(BrokerError::network_violation("unparseable-url", 0));
    };

    if policy::allows_network_connection(manifest, &host, port) {
        InspectResult::Allow
    } else {
        InspectResult::Deny(BrokerError::network_violation(&host, port))
    }
}

/// Recursively collect all string values from a JSON value.
///
/// Descends into objects and arrays, collecting every leaf string.
fn extract_strings(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_strings(value, &mut out);
    out
}

/// Recursive helper that appends string leaves to `out`.
fn collect_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(arr) => {
            for item in arr {
                collect_strings(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_strings(v, out);
            }
        }
        // Numbers, booleans, null — not strings.
        _ => {}
    }
}

/// Parse the host and port from a URL string.
///
/// Returns `None` if the URL cannot be parsed to a host+port pair.
/// Handles `scheme://host:port/path` and `scheme://host/path` (defaults to port 443).
fn parse_host_port(url: &str) -> Option<(String, u16)> {
    // Strip scheme.
    let after_scheme = url.split("://").nth(1)?;
    // Strip path/query/fragment.
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    // Strip userinfo.
    let host_and_port = authority.split('@').last().unwrap_or(authority);

    if let Some(colon_pos) = host_and_port.rfind(':') {
        let host = &host_and_port[..colon_pos];
        let port_str = &host_and_port[colon_pos + 1..];
        let port: u16 = port_str.parse().ok()?;
        Some((host.to_string(), port))
    } else {
        // No explicit port — default based on scheme.
        let scheme = url.split("://").next().unwrap_or("https");
        let default_port = default_port_for_scheme(scheme);
        Some((host_and_port.to_string(), default_port))
    }
}

/// Returns the conventional port for a URL scheme.
fn default_port_for_scheme(scheme: &str) -> u16 {
    match scheme {
        "https" | "wss" => 443,
        "http" | "ws" => 80,
        "ftp" => 21,
        "ssh" | "sftp" => 22,
        _ => 443,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use mcparmor_core::manifest::{ArmorManifest, FilesystemPolicy, NetworkPolicy, Profile};
    use serde_json::json;

    fn manifest_with_fs(read: &[&str], write: &[&str]) -> ArmorManifest {
        ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Sandboxed,
            filesystem: FilesystemPolicy {
                read: read.iter().map(|s| s.to_string()).collect(),
                write: write.iter().map(|s| s.to_string()).collect(),
            },
            network: NetworkPolicy::default(),
            spawn: false,
            env: Default::default(),
            output: Default::default(),
            audit: Default::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        }
    }

    fn manifest_with_network(allow: &[&str], deny_local: bool, deny_metadata: bool) -> ArmorManifest {
        ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Sandboxed,
            filesystem: Default::default(),
            network: NetworkPolicy {
                allow: allow.iter().map(|s| s.to_string()).collect(),
                deny_local,
                deny_metadata,
            },
            spawn: false,
            env: Default::default(),
            output: Default::default(),
            audit: Default::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        }
    }

    fn default_manifest() -> ArmorManifest {
        manifest_with_fs(&[], &[])
    }

    // --- Path traversal unit tests ---

    #[test]
    fn percent_encoded_dot_traversal_is_denied() {
        let manifest = manifest_with_fs(&["/tmp/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "path": "%2e%2e/etc/passwd" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn percent_encoded_slash_traversal_in_url_is_denied() {
        let manifest = manifest_with_network(&["api.example.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://api.example.com/%2e%2e/secret" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn mixed_case_percent_encoded_traversal_is_denied() {
        let manifest = manifest_with_fs(&["/tmp/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read", "arguments": { "path": "%2E%2E%2Fetc%2Fpasswd" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn dotdot_in_filename_without_slash_is_not_traversal() {
        // "release..notes.md" contains ".." but is not a traversal sequence.
        // It should pass the traversal check and be evaluated as a path.
        let manifest = manifest_with_fs(&["/workspace/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read", "arguments": { "path": "/workspace/release..notes.md" } }
        });
        // No traversal, path is in allowlist — must be allowed.
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn version_range_string_is_not_traversal() {
        // A semver range "1.0..2.0" passed as a non-path argument must not be blocked.
        let manifest = manifest_with_fs(&[], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "check_versions", "arguments": { "range": "1.0..2.0" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    // --- Passthrough tests ---

    #[test]
    fn non_tools_call_messages_are_allowed() {
        let manifest = default_manifest();
        let methods = ["initialize", "ping", "tools/list", "notifications/message", ""];
        for method in &methods {
            let msg = json!({ "jsonrpc": "2.0", "method": method, "id": 1 });
            assert!(
                matches!(check_message(&msg, &manifest), InspectResult::Allow),
                "method '{method}' should pass through"
            );
        }
    }

    #[test]
    fn tools_call_with_no_path_or_url_params_is_allowed() {
        let manifest = default_manifest();
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "list_files", "arguments": { "count": 10, "verbose": true } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn tools_call_with_empty_params_is_allowed() {
        let manifest = default_manifest();
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "ping", "arguments": {} }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn tools_call_with_no_arguments_key_is_allowed() {
        let manifest = default_manifest();
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "noop" }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    // --- Path traversal tests ---

    #[test]
    fn path_traversal_is_always_denied_regardless_of_allowlist() {
        let manifest = manifest_with_fs(&["/tmp/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read_file", "arguments": { "path": "../../etc/passwd" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn path_traversal_in_url_is_denied() {
        let manifest = manifest_with_network(&["api.example.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://api.example.com/../secret" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    // --- Filesystem path tests ---

    #[test]
    fn absolute_path_not_in_allowlist_is_denied() {
        let manifest = manifest_with_fs(&["/tmp/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read_file", "arguments": { "path": "/etc/passwd" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn absolute_path_matching_read_glob_is_allowed() {
        let manifest = manifest_with_fs(&["/tmp/mcparmor/*"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read_file", "arguments": { "path": "/tmp/mcparmor/output.txt" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn absolute_path_matching_write_glob_is_allowed() {
        let manifest = manifest_with_fs(&[], &["/workspace/**"]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "write_file", "arguments": { "path": "/workspace/src/main.rs" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn empty_filesystem_allowlist_denies_all_paths() {
        let manifest = manifest_with_fs(&[], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read_file", "arguments": { "path": "/tmp/file.txt" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn home_relative_path_is_inspected() {
        let manifest = manifest_with_fs(&[], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "read_file", "arguments": { "path": "~/Documents/secret.pdf" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    // --- Network URL tests ---

    #[test]
    fn url_to_unlisted_host_is_denied() {
        let manifest = manifest_with_network(&["api.github.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://evil.com/exfil" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn url_to_listed_host_and_port_is_allowed() {
        let manifest = manifest_with_network(&["api.github.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://api.github.com/repos" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn url_to_wrong_port_is_denied() {
        let manifest = manifest_with_network(&["api.github.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "http://api.github.com/repos" } }
        });
        // Default port for http is 80, but rule only allows :443.
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn localhost_url_denied_when_deny_local_true() {
        let manifest = manifest_with_network(&["localhost:8080"], true, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "http://localhost:8080/api" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn localhost_url_allowed_when_deny_local_false() {
        let manifest = manifest_with_network(&["localhost:8080"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "http://localhost:8080/api" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn loopback_ip_url_denied_when_deny_local_true() {
        let manifest = manifest_with_network(&["127.0.0.1:9000"], true, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "http://127.0.0.1:9000/data" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn metadata_url_denied_when_deny_metadata_true() {
        let manifest = manifest_with_network(&["169.254.169.254:80"], false, true);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "http://169.254.169.254/latest/meta-data/" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn wildcard_host_rule_matches_any_host() {
        let manifest = manifest_with_network(&["*:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://anything.example.com/path" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn subdomain_wildcard_rule_matches_subdomain() {
        let manifest = manifest_with_network(&["*.github.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://api.github.com/repos" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn subdomain_wildcard_rule_does_not_match_unrelated_domain() {
        let manifest = manifest_with_network(&["*.github.com:443"], false, false);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "fetch", "arguments": { "url": "https://evil.com/path" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    // --- Deeply nested / mixed-type params ---

    #[test]
    fn deeply_nested_json_params_are_inspected() {
        let manifest = manifest_with_fs(&[], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": {
                "name": "process",
                "arguments": {
                    "outer": {
                        "inner": {
                            "deep": "/etc/shadow"
                        }
                    }
                }
            }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn non_string_values_do_not_trigger_inspection() {
        let manifest = manifest_with_fs(&[], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": {
                "name": "compute",
                "arguments": {
                    "count": 42,
                    "enabled": false,
                    "ratio": 3.14,
                    "tags": [1, 2, 3],
                    "meta": null
                }
            }
        });
        // All values are non-string — no path/URL inspection triggered.
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn array_of_paths_all_checked() {
        let manifest = manifest_with_fs(&["/allowed/**"], &[]);
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": {
                "name": "batch_read",
                "arguments": {
                    "paths": ["/allowed/file.txt", "/etc/passwd"]
                }
            }
        });
        // Second path is not in allowlist.
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Deny(_)));
    }

    #[test]
    fn empty_string_value_does_not_panic() {
        let manifest = default_manifest();
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "name": "noop", "arguments": { "label": "" } }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn unicode_and_special_chars_in_params_do_not_panic() {
        let manifest = default_manifest();
        let msg = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": {
                "name": "noop",
                "arguments": {
                    "label": "hello 世界 \0 \n\t",
                    "braces": "{ } [ ] \" \\"
                }
            }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn message_with_no_params_key_is_allowed() {
        let manifest = default_manifest();
        let msg = json!({ "jsonrpc": "2.0", "method": "tools/call", "id": 1 });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }

    #[test]
    fn message_that_is_a_response_not_a_request_is_allowed() {
        let manifest = default_manifest();
        // Responses have no "method" field.
        let msg = json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "content": [{ "type": "text", "text": "/etc/passwd" }] }
        });
        assert!(matches!(check_message(&msg, &manifest), InspectResult::Allow));
    }
}
