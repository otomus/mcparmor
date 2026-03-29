//! Capability enforcement decisions — pure manifest policy checks.
//!
//! Provides stateless, pure functions that answer "may the tool do X?"
//! given only the `ArmorManifest`. These functions form the single source of
//! truth for capability decisions across the broker and all language SDKs.
//!
//! All functions are free of side effects: they take manifest references and
//! return booleans. The broker's Layer 1 inspector and the OS sandbox layer
//! both delegate their allow/deny decisions to these functions.

use crate::manifest::{ArmorManifest, NetworkPolicy};
use glob::Pattern;

/// Returns `true` if the manifest permits a network connection to `host:port`.
///
/// Deny rules are evaluated before the allow-list:
/// 1. `deny_local` blocks loopback addresses and `localhost`
/// 2. `deny_metadata` blocks cloud metadata endpoints (`169.254.x.x`)
/// 3. The connection must match at least one rule in `network.allow`
///
/// An empty `network.allow` list denies all connections after the deny rules.
///
/// # Arguments
/// * `manifest` - The parsed armor manifest
/// * `host` - The target hostname or IP address
/// * `port` - The target TCP port number
pub fn allows_network_connection(manifest: &ArmorManifest, host: &str, port: u16) -> bool {
    check_network_allow(host, port, &manifest.network)
}

/// Returns `true` if the manifest permits reading the file at `path`.
///
/// The path must match at least one glob pattern in `filesystem.read`.
/// An empty list denies all read access.
///
/// # Arguments
/// * `manifest` - The parsed armor manifest
/// * `path` - The filesystem path to check
pub fn allows_path_read(manifest: &ArmorManifest, path: &str) -> bool {
    manifest.filesystem.read.iter().any(|pat| glob_matches(pat, path))
}

/// Returns `true` if the manifest permits writing the file at `path`.
///
/// The path must match at least one glob pattern in `filesystem.write`.
/// An empty list denies all write access.
///
/// # Arguments
/// * `manifest` - The parsed armor manifest
/// * `path` - The filesystem path to check
pub fn allows_path_write(manifest: &ArmorManifest, path: &str) -> bool {
    manifest.filesystem.write.iter().any(|pat| glob_matches(pat, path))
}

/// Returns `true` if the manifest permits the tool to spawn child processes.
///
/// # Arguments
/// * `manifest` - The parsed armor manifest
pub fn allows_spawn(manifest: &ArmorManifest) -> bool {
    manifest.spawn
}

/// Returns `true` if the manifest is locked against profile overrides.
///
/// When locked, the broker ignores any `--profile` flag passed to
/// `mcparmor run`. This is a cooperative lock — not enforced at the kernel
/// level.
///
/// # Arguments
/// * `manifest` - The parsed armor manifest
pub fn is_locked(manifest: &ArmorManifest) -> bool {
    manifest.locked
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `host:port` satisfies the network policy's deny rules
/// and allow-list.
fn check_network_allow(host: &str, port: u16, policy: &NetworkPolicy) -> bool {
    if policy.deny_local && is_local_address(host) {
        return false;
    }
    if policy.deny_metadata && is_metadata_address(host) {
        return false;
    }
    policy.allow.iter().any(|rule| network_rule_matches(rule, host, port))
}

/// Returns `true` if the host is a loopback or localhost address.
///
/// Covers: `localhost`, `127.0.0.0/8`, `::1`, and `0.0.0.0`.
pub(crate) fn is_local_address(host: &str) -> bool {
    if host == "localhost" || host == "::1" || host == "0.0.0.0" {
        return true;
    }
    // 127.0.0.0/8 — any address in this range is loopback.
    host.starts_with("127.")
}

/// Returns `true` if the host is a cloud metadata endpoint (`169.254.x.x`).
pub(crate) fn is_metadata_address(host: &str) -> bool {
    host.starts_with("169.254.")
}

/// Returns `true` if the `HOST:PORT` network rule matches the given host and port.
///
/// Rules have the form `HOST:PORT` where HOST can be `*`, `*.domain`, or an
/// exact hostname, and PORT can be `*` or an explicit port number.
pub(crate) fn network_rule_matches(rule: &str, host: &str, port: u16) -> bool {
    let Some(colon_pos) = rule.rfind(':') else {
        return false;
    };
    let rule_host = &rule[..colon_pos];
    let rule_port = &rule[colon_pos + 1..];
    network_host_matches(rule_host, host) && network_port_matches(rule_port, port)
}

/// Returns `true` if the rule host pattern matches the given host.
///
/// Supports three forms:
/// - `*` — matches any host
/// - `*.domain` — matches `domain` itself and all subdomains
/// - exact string — must match literally
fn network_host_matches(rule_host: &str, host: &str) -> bool {
    if rule_host == "*" {
        return true;
    }
    if let Some(suffix) = rule_host.strip_prefix("*.") {
        return host.ends_with(suffix) || host == suffix;
    }
    rule_host == host
}

/// Returns `true` if the rule port pattern matches the given port.
///
/// `*` matches any port; otherwise the string must parse as a `u16` equal to `port`.
fn network_port_matches(rule_port: &str, port: u16) -> bool {
    if rule_port == "*" {
        return true;
    }
    rule_port.parse::<u16>().map(|p| p == port).unwrap_or(false)
}

/// Returns `true` if `path` matches the glob `pattern`.
///
/// Uses `require_literal_separator: true` so that a single `*` wildcard
/// matches only within a single path component (never across `/`). This is
/// the correct security-enforcement behaviour: a manifest declaring
/// `filesystem.read: ["/tmp/*"]` must not allow access to `/tmp/subdir/file`.
///
/// `**` still matches any sequence of path components, including separators.
///
/// An invalid glob pattern is treated as a non-match (returns `false`)
/// rather than panicking, so malformed manifests degrade gracefully.
pub(crate) fn glob_matches(pattern: &str, path: &str) -> bool {
    use glob::MatchOptions;
    Pattern::new(pattern)
        .map(|p| {
            p.matches_with(
                path,
                MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: true,
                    require_literal_leading_dot: false,
                },
            )
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        ArmorManifest, AuditPolicy, EnvPolicy, FilesystemPolicy, NetworkPolicy, OutputPolicy,
        Profile,
    };

    // ---------------------------------------------------------------------------
    // Test manifest builders
    // ---------------------------------------------------------------------------

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
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        }
    }

    fn manifest_with_network(allow: &[&str], deny_local: bool, deny_metadata: bool) -> ArmorManifest {
        ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Sandboxed,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy {
                allow: allow.iter().map(|s| s.to_string()).collect(),
                deny_local,
                deny_metadata,
            },
            spawn: false,
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        }
    }

    fn manifest_with_flags(spawn: bool, locked: bool) -> ArmorManifest {
        ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Sandboxed,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy::default(),
            spawn,
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked,
            min_spec: None,
        }
    }

    // ---------------------------------------------------------------------------
    // allows_network_connection
    // ---------------------------------------------------------------------------

    #[test]
    fn network_exact_host_and_port_is_allowed() {
        let m = manifest_with_network(&["api.github.com:443"], false, false);
        assert!(allows_network_connection(&m, "api.github.com", 443));
    }

    #[test]
    fn network_wrong_port_is_denied() {
        let m = manifest_with_network(&["api.github.com:443"], false, false);
        assert!(!allows_network_connection(&m, "api.github.com", 80));
    }

    #[test]
    fn network_unlisted_host_is_denied() {
        let m = manifest_with_network(&["api.github.com:443"], false, false);
        assert!(!allows_network_connection(&m, "evil.com", 443));
    }

    #[test]
    fn network_empty_allow_list_denies_all() {
        let m = manifest_with_network(&[], false, false);
        assert!(!allows_network_connection(&m, "anything.example.com", 443));
    }

    #[test]
    fn network_wildcard_host_matches_any_host() {
        let m = manifest_with_network(&["*:443"], false, false);
        assert!(allows_network_connection(&m, "anything.example.com", 443));
    }

    #[test]
    fn network_wildcard_port_matches_any_port() {
        let m = manifest_with_network(&["api.example.com:*"], false, false);
        assert!(allows_network_connection(&m, "api.example.com", 8080));
        assert!(allows_network_connection(&m, "api.example.com", 443));
    }

    #[test]
    fn network_subdomain_wildcard_matches_subdomain() {
        let m = manifest_with_network(&["*.github.com:443"], false, false);
        assert!(allows_network_connection(&m, "api.github.com", 443));
    }

    #[test]
    fn network_subdomain_wildcard_matches_apex_domain() {
        // *.github.com should match github.com itself per spec.
        let m = manifest_with_network(&["*.github.com:443"], false, false);
        assert!(allows_network_connection(&m, "github.com", 443));
    }

    #[test]
    fn network_subdomain_wildcard_does_not_match_unrelated_domain() {
        let m = manifest_with_network(&["*.github.com:443"], false, false);
        assert!(!allows_network_connection(&m, "evil.com", 443));
    }

    #[test]
    fn network_localhost_denied_when_deny_local_true() {
        let m = manifest_with_network(&["localhost:8080"], true, false);
        assert!(!allows_network_connection(&m, "localhost", 8080));
    }

    #[test]
    fn network_localhost_allowed_when_deny_local_false() {
        let m = manifest_with_network(&["localhost:8080"], false, false);
        assert!(allows_network_connection(&m, "localhost", 8080));
    }

    #[test]
    fn network_loopback_ip_denied_when_deny_local_true() {
        let m = manifest_with_network(&["127.0.0.1:9000"], true, false);
        assert!(!allows_network_connection(&m, "127.0.0.1", 9000));
    }

    #[test]
    fn network_loopback_127_range_denied_when_deny_local_true() {
        let m = manifest_with_network(&["127.100.0.1:80"], true, false);
        assert!(!allows_network_connection(&m, "127.100.0.1", 80));
    }

    #[test]
    fn network_ipv6_loopback_denied_when_deny_local_true() {
        let m = manifest_with_network(&["::1:8080"], true, false);
        assert!(!allows_network_connection(&m, "::1", 8080));
    }

    #[test]
    fn network_metadata_denied_when_deny_metadata_true() {
        let m = manifest_with_network(&["169.254.169.254:80"], false, true);
        assert!(!allows_network_connection(&m, "169.254.169.254", 80));
    }

    #[test]
    fn network_metadata_allowed_when_deny_metadata_false() {
        let m = manifest_with_network(&["169.254.169.254:80"], false, false);
        assert!(allows_network_connection(&m, "169.254.169.254", 80));
    }

    #[test]
    fn network_rule_without_colon_is_not_matched() {
        // A malformed rule (no colon) must not panic and must not match.
        let m = manifest_with_network(&["no-colon-rule"], false, false);
        assert!(!allows_network_connection(&m, "no-colon-rule", 80));
    }

    #[test]
    fn network_rule_with_invalid_port_number_is_not_matched() {
        let m = manifest_with_network(&["api.example.com:notaport"], false, false);
        assert!(!allows_network_connection(&m, "api.example.com", 443));
    }

    #[test]
    fn network_empty_host_does_not_panic() {
        let m = manifest_with_network(&["*:443"], false, false);
        // Empty host should not crash; local-address check handles it.
        let _ = allows_network_connection(&m, "", 443);
    }

    // ---------------------------------------------------------------------------
    // allows_path_read
    // ---------------------------------------------------------------------------

    #[test]
    fn path_read_matches_exact_pattern() {
        let m = manifest_with_fs(&["/tmp/output.txt"], &[]);
        assert!(allows_path_read(&m, "/tmp/output.txt"));
    }

    #[test]
    fn path_read_matches_glob_pattern() {
        let m = manifest_with_fs(&["/tmp/**"], &[]);
        assert!(allows_path_read(&m, "/tmp/subdir/file.txt"));
    }

    #[test]
    fn path_read_denies_path_outside_allowlist() {
        let m = manifest_with_fs(&["/tmp/**"], &[]);
        assert!(!allows_path_read(&m, "/etc/passwd"));
    }

    #[test]
    fn path_read_empty_list_denies_all() {
        let m = manifest_with_fs(&[], &[]);
        assert!(!allows_path_read(&m, "/tmp/file.txt"));
    }

    #[test]
    fn path_read_invalid_glob_pattern_returns_false() {
        // Malformed glob (unmatched bracket) must not panic.
        let m = manifest_with_fs(&["[invalid"], &[]);
        assert!(!allows_path_read(&m, "/tmp/file.txt"));
    }

    #[test]
    fn path_read_empty_path_does_not_panic() {
        let m = manifest_with_fs(&["/tmp/**"], &[]);
        let _ = allows_path_read(&m, "");
    }

    #[test]
    fn path_read_unicode_path_does_not_panic() {
        let m = manifest_with_fs(&["/tmp/**"], &[]);
        let _ = allows_path_read(&m, "/tmp/\u{4e2d}\u{6587}/file.txt");
    }

    // ---------------------------------------------------------------------------
    // allows_path_write
    // ---------------------------------------------------------------------------

    #[test]
    fn path_write_matches_glob_pattern() {
        let m = manifest_with_fs(&[], &["/workspace/**"]);
        assert!(allows_path_write(&m, "/workspace/src/main.rs"));
    }

    #[test]
    fn path_write_denies_path_not_in_write_list() {
        let m = manifest_with_fs(&[], &["/workspace/**"]);
        assert!(!allows_path_write(&m, "/etc/passwd"));
    }

    #[test]
    fn path_write_empty_list_denies_all() {
        let m = manifest_with_fs(&[], &[]);
        assert!(!allows_path_write(&m, "/workspace/file.txt"));
    }

    #[test]
    fn path_write_does_not_use_read_list() {
        // A path in the read list must NOT be returned as writable.
        let m = manifest_with_fs(&["/tmp/**"], &[]);
        assert!(!allows_path_write(&m, "/tmp/file.txt"));
    }

    #[test]
    fn path_read_does_not_use_write_list() {
        // A path in the write list must NOT be returned as readable.
        let m = manifest_with_fs(&[], &["/workspace/**"]);
        assert!(!allows_path_read(&m, "/workspace/src/main.rs"));
    }

    // ---------------------------------------------------------------------------
    // allows_spawn
    // ---------------------------------------------------------------------------

    #[test]
    fn spawn_returns_true_when_manifest_allows_it() {
        let m = manifest_with_flags(true, false);
        assert!(allows_spawn(&m));
    }

    #[test]
    fn spawn_returns_false_when_manifest_denies_it() {
        let m = manifest_with_flags(false, false);
        assert!(!allows_spawn(&m));
    }

    // ---------------------------------------------------------------------------
    // is_locked
    // ---------------------------------------------------------------------------

    #[test]
    fn is_locked_returns_true_when_manifest_is_locked() {
        let m = manifest_with_flags(false, true);
        assert!(is_locked(&m));
    }

    #[test]
    fn is_locked_returns_false_when_manifest_is_unlocked() {
        let m = manifest_with_flags(false, false);
        assert!(!is_locked(&m));
    }

    // ---------------------------------------------------------------------------
    // Internal helper edge cases
    // ---------------------------------------------------------------------------

    #[test]
    fn is_local_address_recognises_localhost() {
        assert!(is_local_address("localhost"));
    }

    #[test]
    fn is_local_address_recognises_ipv4_loopback() {
        assert!(is_local_address("127.0.0.1"));
        assert!(is_local_address("127.255.255.255"));
    }

    #[test]
    fn is_local_address_recognises_ipv6_loopback() {
        assert!(is_local_address("::1"));
    }

    #[test]
    fn is_local_address_does_not_match_public_ip() {
        assert!(!is_local_address("8.8.8.8"));
        assert!(!is_local_address("192.168.1.1"));
    }

    #[test]
    fn is_metadata_address_recognises_169_254_range() {
        assert!(is_metadata_address("169.254.169.254"));
        assert!(is_metadata_address("169.254.0.1"));
    }

    #[test]
    fn is_metadata_address_does_not_match_other_ranges() {
        assert!(!is_metadata_address("192.168.1.1"));
        assert!(!is_metadata_address("10.0.0.1"));
    }

    #[test]
    fn network_rule_matches_exact_host_and_port() {
        assert!(network_rule_matches("api.example.com:443", "api.example.com", 443));
    }

    #[test]
    fn network_rule_rejects_mismatched_port() {
        assert!(!network_rule_matches("api.example.com:443", "api.example.com", 80));
    }

    #[test]
    fn network_rule_rejects_rule_with_no_colon() {
        assert!(!network_rule_matches("no-colon", "no-colon", 80));
    }

    #[test]
    fn glob_matches_single_star_wildcard() {
        assert!(glob_matches("/tmp/*", "/tmp/file.txt"));
        assert!(!glob_matches("/tmp/*", "/tmp/subdir/file.txt"));
    }

    #[test]
    fn glob_matches_double_star_wildcard() {
        assert!(glob_matches("/tmp/**", "/tmp/subdir/deep/file.txt"));
    }

    #[test]
    fn glob_invalid_pattern_returns_false_without_panic() {
        assert!(!glob_matches("[unclosed", "/tmp/file.txt"));
    }
}
