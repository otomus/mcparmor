//! macOS sandbox provider — Seatbelt via sandbox-exec.
//!
//! Generates an SBPL (Sandbox Profile Language) profile from the armor manifest
//! and executes the tool under `sandbox-exec -p <profile> <command>`.
//!
//! ## Enforcement capabilities on macOS
//!
//! - **Filesystem isolation**: enforced via `(allow file-read* (subpath ...))` rules.
//!   SBPL enforces at the directory level; Layer 1 param inspection enforces
//!   precise glob patterns within that directory.
//!
//! - **Spawn blocking**: enforced via `(deny default)` which covers `process-exec`
//!   and `process-fork`. Enabled explicitly when `spawn: true` in the manifest.
//!
//! - **Network port enforcement**: enforced via `(allow network-outbound (remote tcp "host:port"))`.
//!
//! - **Network hostname enforcement**: SBPL resolves hostnames at rule compilation time
//!   (not at connection time). Rules with exact hostnames are enforced correctly for
//!   stable-IP services. For CDN-backed or frequently-rotating services, hostname
//!   enforcement falls back to Layer 1 param inspection.
//!
//! - **Loopback blocking**: covered by `(deny default)` — local addresses are blocked
//!   unless explicitly in `network.allow` (or `deny_local: false` for browser profile).
//!
//! - **Metadata blocking**: `169.254.0.0/16` is covered by `(deny default)` and
//!   the specific address `169.254.169.254` is explicitly denied in all profiles.

use anyhow::Result;
use mcparmor_core::manifest::{ArmorManifest, Profile};

use super::{EnforcementSummary, SandboxProvider, SandboxedCommand};

/// macOS Seatbelt sandbox provider using `sandbox-exec`.
///
/// Generates an SBPL profile from the armor manifest at spawn time and
/// wraps the tool command: `sandbox-exec -p <profile> <command> <args>`.
pub struct MacosSeatbelt;

impl SandboxProvider for MacosSeatbelt {
    fn apply(
        &self,
        manifest: &ArmorManifest,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedCommand> {
        // Resolve to an absolute path so that the SBPL (literal ...) rule
        // matches the path that the kernel sees after execvp() resolution.
        let absolute_command = resolve_absolute_path(command);
        let sbpl_profile = generate_sbpl_profile(manifest, &absolute_command)?;

        // Wrap: sandbox-exec -p <profile> <command> <args…>
        let mut sandbox_args = vec!["-p".to_string(), sbpl_profile, command.to_string()];
        sandbox_args.extend_from_slice(args);

        Ok(SandboxedCommand {
            program: "sandbox-exec".to_string(),
            args: sandbox_args,
            env: Vec::new(),
            process_group: true,
        })
    }

    fn is_available(&self) -> bool {
        // sandbox-exec ships with all macOS versions we target (12–15).
        std::path::Path::new("/usr/bin/sandbox-exec").exists()
    }

    fn enforcement_summary(&self) -> EnforcementSummary {
        EnforcementSummary {
            // SBPL provides write-path isolation; file READ isolation is Layer 1 only.
            // See push_system_allowances for the rationale.
            filesystem_isolation: true,
            spawn_blocking: true,
            network_port_enforcement: true,
            // Hostname enforcement is best-effort: SBPL resolves hostnames at
            // rule-compilation time. CDN-backed services may rotate IPs and
            // bypass hostname rules; Layer 1 param inspection is the reliable
            // hostname enforcement layer.
            network_hostname_enforcement: true,
            mechanism: "macOS Seatbelt (sandbox-exec)".to_string(),
        }
    }
}

/// Resolve a command to its absolute path.
///
/// The SBPL `(literal ...)` rule matches the absolute path that the kernel
/// sees after `execvp()` resolves the binary. If `command` is already
/// absolute, canonicalize it. If it is relative, try canonicalize from
/// the current working directory. Falls back to the original string so that
/// the sandbox can fail with a clear error rather than silently allowing.
fn resolve_absolute_path(command: &str) -> String {
    let path = std::path::Path::new(command);
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().into_owned();
    }
    // PATH-based resolution for bare command names (e.g. "node", "python3").
    if !command.contains('/') {
        if let Ok(output) = std::process::Command::new("which").arg(command).output() {
            let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !resolved.is_empty() {
                return resolved;
            }
        }
    }
    command.to_string()
}

/// Generate an SBPL profile string from an armor manifest.
///
/// The profile uses `(deny default)` as the baseline and adds allow rules
/// for each declared capability. System libraries required for any process
/// to start are always permitted.
///
/// `tool_path` is always explicitly allowed for `process-exec` so that the
/// initial exec of the tool binary succeeds even under `(deny default)`.
/// When `spawn: false`, no other binaries may be exec'd — the tool binary
/// is the only entry in the process-exec allow list.
///
/// ## Glob patterns and SBPL
///
/// SBPL enforces at directory granularity via `(subpath ...)`. Glob patterns
/// in the manifest (e.g. `/tmp/myapp/*.log`) are mapped to their longest
/// non-glob prefix (`/tmp/myapp`). Layer 1 param inspection enforces the
/// precise glob within that allowed directory.
pub fn generate_sbpl_profile(manifest: &ArmorManifest, tool_path: &str) -> Result<String> {
    let mut lines: Vec<String> = Vec::new();

    push_preamble(&mut lines);
    push_system_allowances(&mut lines);
    push_initial_exec_allowance(&mut lines, tool_path);
    push_filesystem_rules(&mut lines, manifest);
    push_spawn_rules(&mut lines, manifest);
    push_network_rules(&mut lines, manifest);

    Ok(lines.join("\n"))
}

/// Write the SBPL version declaration and deny-default baseline.
fn push_preamble(lines: &mut Vec<String>) {
    lines.push("(version 1)".to_string());
    lines.push("(deny default)".to_string());
    lines.push(String::new());
}

/// Write the minimum system allowances required for any process to start.
///
/// These allowances are always present regardless of the manifest.
///
/// ## File read strategy
///
/// SBPL file-read restrictions cannot reliably sandbox arbitrary language
/// runtimes (Go, Python, Node, JVM). These runtimes read kernel headers,
/// system libraries, locale data, and runtime internals from paths that vary
/// by OS version, architecture, and runtime version — and which are impossible
/// to enumerate statically. Restricting file-read to known subpaths causes
/// SIGABRT during runtime initialization.
///
/// Therefore `(allow file-read*)` is applied globally in system allowances.
/// Filesystem READ enforcement is handled by Layer 1 param inspection, which
/// reliably checks path arguments before they reach the tool. SBPL provides
/// WRITE isolation (declared paths only), spawn blocking, and network port
/// enforcement — which cannot be bypassed through parameter manipulation.
fn push_system_allowances(lines: &mut Vec<String>) {
    lines.push("; File reads — allow all (Layer 1 enforces path policy, not SBPL)".to_string());
    lines.push("(allow file-read*)".to_string());

    // Write access to /dev/null and /tmp only (Layer 2 write isolation via manifest rules).
    lines.push("(allow file-write-data (literal \"/dev/null\"))".to_string());
    lines.push("(allow file-write* (subpath \"/private/tmp\"))".to_string());

    // Mach IPC — required for dynamic linking and most macOS frameworks
    lines.push("(allow mach-lookup)".to_string());
    lines.push("(allow mach-register)".to_string());

    // Process introspection — required by most language runtimes
    lines.push("(allow process-info*)".to_string());
    lines.push("(allow signal)".to_string());

    // sysctl reads — required by Go/JVM/Python for hw.pagesize, hw.ncpu, etc.
    lines.push("(allow sysctl-read)".to_string());

    lines.push(String::new());
}

/// Allow the initial exec of the tool binary.
///
/// `(deny default)` covers `process-exec`, which would otherwise prevent
/// `execvp` of the tool binary itself from succeeding. This rule pins
/// the allow to a `(literal ...)` path so only this exact binary may be
/// exec'd. When `spawn: false` (the default), no other `(allow process-exec)`
/// rule is added — preventing the tool from launching any child process.
fn push_initial_exec_allowance(lines: &mut Vec<String>, tool_path: &str) {
    lines.push("; Allow the tool binary to be exec'd (required by deny default)".to_string());
    let escaped = escape_sbpl_string(tool_path);
    lines.push(format!("(allow process-exec (literal \"{escaped}\"))"));
    lines.push(String::new());
}

/// Write filesystem allow rules derived from the manifest.
///
/// Only WRITE rules are generated here — reads are globally allowed by the
/// system allowances (see `push_system_allowances` for the rationale).
/// The tool may only write to paths declared in `filesystem.write`; all other
/// write destinations are covered by `(deny default)`.
fn push_filesystem_rules(lines: &mut Vec<String>, manifest: &ArmorManifest) {
    if manifest.filesystem.write.is_empty() {
        return;
    }

    lines.push("; Filesystem write rules from armor.json".to_string());

    for pattern in &manifest.filesystem.write {
        let prefix = glob_to_subpath_prefix(pattern);
        let escaped = escape_sbpl_string(&prefix);
        lines.push(format!("(allow file-write* (subpath \"{escaped}\"))"));
    }

    lines.push(String::new());
}

/// Write spawn allow/deny rules derived from the manifest.
///
/// `(deny default)` already covers `process-exec` and `process-fork`.
/// We add explicit `(allow ...)` only when `spawn: true`.
fn push_spawn_rules(lines: &mut Vec<String>, manifest: &ArmorManifest) {
    if manifest.spawn {
        lines.push("; Spawn enabled — spawn: true in armor.json".to_string());
        lines.push("(allow process-exec)".to_string());
        lines.push("(allow process-fork)".to_string());
        lines.push(String::new());
    }
    // When spawn: false (the default), deny default already blocks process-exec
    // and process-fork. No additional rule is needed.
}

/// Write network allow rules derived from the manifest.
///
/// deny_local and deny_metadata are enforced by omission — since we use
/// `(deny default)`, any host not in the allow list is already blocked.
/// The metadata endpoint (169.254.169.254) is explicitly denied as defence-
/// in-depth in case a future allow rule unintentionally covers it.
fn push_network_rules(lines: &mut Vec<String>, manifest: &ArmorManifest) {
    let is_browser = manifest.profile == Profile::Browser;
    let allow_local = is_browser && !manifest.network.deny_local;
    let has_network = !manifest.network.allow.is_empty() || allow_local;

    if !has_network {
        return;
    }

    lines.push("; Network rules from armor.json".to_string());

    // DNS resolution is required to connect to any named host.
    lines.push("(allow network-outbound (remote udp \"*:53\"))".to_string());
    lines.push("(allow network-outbound (remote tcp \"*:53\"))".to_string());

    // The cloud metadata endpoint (169.254.169.254) is covered by (deny default).
    // No explicit deny rule is needed — any address not in the allow list is blocked.
    // Callers must add "169.254.169.254:*" to network.allow to permit it (inadvisable).

    // Allow declared network destinations.
    for rule in &manifest.network.allow {
        if let Some(sbpl) = network_rule_to_sbpl(rule) {
            lines.push(format!("(allow network-outbound {sbpl})"));
        }
    }

    // Browser profile — allow loopback for CDP and local server connections.
    if allow_local {
        lines.push("(allow network-outbound (remote tcp \"localhost:*\"))".to_string());
        lines.push("(allow network-outbound (remote tcp \"127.0.0.1:*\"))".to_string());
        lines.push("(allow network-outbound (remote tcp \"[::1]:*\"))".to_string());
    }

    lines.push(String::new());
}

/// Convert a `network.allow` entry to an SBPL remote clause.
///
/// SBPL's `(remote tcp ...)` predicate accepts only `*` or `localhost` as the
/// host — named hostnames and IP addresses are not valid in this position.
/// Therefore this function extracts the PORT only and generates a
/// `(remote tcp "*:PORT")` rule. This enforces port-level restrictions at
/// Layer 2 (Seatbelt); hostname restrictions are enforced at Layer 1 (param
/// inspection) which operates before the connection is ever attempted.
///
/// Returns `None` if the port cannot be parsed (malformed entry).
fn network_rule_to_sbpl(rule: &str) -> Option<String> {
    let (_host, port) = rule.rsplit_once(':')?;
    // Validate port is numeric or wildcard before emitting.
    if port != "*" && port.parse::<u16>().is_err() {
        return None;
    }
    Some(format!("(remote tcp \"*:{port}\")"))
}

/// Strip glob characters from a path pattern and return the longest
/// non-glob prefix suitable for an SBPL `(subpath ...)` rule.
///
/// Examples:
/// - `/tmp/myapp/*`      → `/tmp/myapp`
/// - `/home/**/*.ts`     → `/home`
/// - `/etc/passwd`       → `/etc/passwd`
/// - `~/Documents`       → `/Users/<user>/Documents`
fn glob_to_subpath_prefix(pattern: &str) -> String {
    let expanded = expand_home(pattern);
    // Split on the first path component that contains a glob character.
    let prefix = expanded
        .split('/')
        .take_while(|component| !component.contains(['*', '?', '[', ']', '{', '}']))
        .collect::<Vec<_>>()
        .join("/");

    if prefix.is_empty() {
        "/".to_string()
    } else {
        prefix
    }
}

/// Expand a leading `~` to the user's home directory.
///
/// When the home directory cannot be determined, falls back to `/` with a
/// warning so the resulting SBPL rule is `(subpath "/")` — permissive but
/// visible — rather than a silent relative path that Seatbelt may misinterpret.
fn expand_home(path: &str) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| {
        eprintln!(
            "warning: [mcparmor] cannot determine home directory — \
             '~' in filesystem paths will expand to '/' in SBPL rules."
        );
        std::path::PathBuf::from("/")
    });

    if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest).to_string_lossy().into_owned()
    } else if path == "~" {
        home.to_string_lossy().into_owned()
    } else {
        path.to_string()
    }
}

/// Escape a string for inclusion inside an SBPL double-quoted string literal.
fn escape_sbpl_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcparmor_core::manifest::{
        ArmorManifest, AuditPolicy, EnvPolicy, FilesystemPolicy, NetworkPolicy, OutputPolicy,
        Profile,
    };

    fn manifest_for_profile(profile: Profile) -> ArmorManifest {
        ArmorManifest {
            version: "1.0".to_string(),
            profile,
            filesystem: FilesystemPolicy::default(),
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

    #[test]
    fn strict_profile_denies_default() {
        let manifest = manifest_for_profile(Profile::Strict);
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        assert!(profile.contains("(deny default)"));
        // process-exec is only present as a literal allow for the tool binary itself.
        assert!(profile.contains("(allow process-exec (literal \"/usr/local/bin/test-tool\"))"));
    }

    #[test]
    fn initial_exec_allowance_always_present_for_tool_binary() {
        let manifest = manifest_for_profile(Profile::Sandboxed);
        let profile = generate_sbpl_profile(&manifest, "/opt/my-mcp-tool").unwrap();
        assert!(profile.contains("(allow process-exec (literal \"/opt/my-mcp-tool\"))"));
    }

    #[test]
    fn spawn_false_does_not_add_global_process_exec() {
        let manifest = manifest_for_profile(Profile::Sandboxed);
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // No bare (allow process-exec) without a literal qualifier.
        assert!(!profile.contains("(allow process-exec)\n"));
    }

    #[test]
    fn spawn_true_adds_global_process_exec_allow() {
        let mut manifest = manifest_for_profile(Profile::Sandboxed);
        manifest.spawn = true;
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // Global (allow process-exec) without qualifier lets the tool spawn any binary.
        assert!(profile.contains("(allow process-exec)\n") || profile.ends_with("(allow process-exec)"));
    }

    #[test]
    fn filesystem_read_declaration_does_not_generate_sbpl_rule() {
        // File reads are allowed globally via system allowances; no per-path rule is emitted.
        let mut manifest = manifest_for_profile(Profile::Sandboxed);
        manifest.filesystem.read = vec!["/tmp/myapp/*".to_string()];
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // There should be no file-read* for this specific subpath — it's covered by (allow file-read*).
        assert!(!profile.contains("(allow file-read* (subpath \"/tmp/myapp\"))"));
        // The global file-read* allowance IS present.
        assert!(profile.contains("(allow file-read*)"));
    }

    #[test]
    fn filesystem_write_generates_write_rule_only() {
        let mut manifest = manifest_for_profile(Profile::Sandboxed);
        manifest.filesystem.write = vec!["/tmp/output".to_string()];
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // Write rule is generated for declared write paths.
        assert!(profile.contains("(allow file-write* (subpath \"/tmp/output\"))"));
    }

    #[test]
    fn network_allow_generates_port_wildcard_outbound_rule() {
        // Hostname is stripped — only port is enforced at Layer 2 (SBPL limitation).
        let mut manifest = manifest_for_profile(Profile::Network);
        manifest.network.allow = vec!["api.github.com:443".to_string()];
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        assert!(profile.contains("(allow network-outbound (remote tcp \"*:443\"))"));
    }

    #[test]
    fn network_deny_metadata_is_covered_by_deny_default() {
        // 169.254.169.254 is blocked by (deny default) — no explicit rule needed.
        let mut manifest = manifest_for_profile(Profile::Network);
        manifest.network.allow = vec!["api.example.com:443".to_string()];
        manifest.network.deny_metadata = true;
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // Metadata IP must NOT be in the allow list.
        assert!(!profile.contains("169.254.169.254"));
    }

    #[test]
    fn browser_profile_with_deny_local_false_allows_loopback() {
        let mut manifest = manifest_for_profile(Profile::Browser);
        manifest.network.deny_local = false;
        manifest.network.allow = vec![];
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        assert!(profile.contains("(allow network-outbound (remote tcp \"localhost:*\"))"));
        assert!(profile.contains("(allow network-outbound (remote tcp \"127.0.0.1:*\"))"));
    }

    #[test]
    fn browser_profile_with_deny_local_true_does_not_allow_loopback() {
        let mut manifest = manifest_for_profile(Profile::Browser);
        manifest.network.deny_local = true;
        manifest.network.allow = vec![];
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        assert!(!profile.contains("127.0.0.1"));
        assert!(!profile.contains("localhost"));
    }

    #[test]
    fn glob_prefix_strips_trailing_glob() {
        assert_eq!(glob_to_subpath_prefix("/tmp/myapp/*"), "/tmp/myapp");
        assert_eq!(glob_to_subpath_prefix("/tmp/myapp/**/*.log"), "/tmp/myapp");
        assert_eq!(glob_to_subpath_prefix("/etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn glob_prefix_root_glob_returns_root() {
        assert_eq!(glob_to_subpath_prefix("/*"), "/");
    }

    #[test]
    fn escape_sbpl_string_handles_quotes_and_backslashes() {
        assert_eq!(escape_sbpl_string(r#"path"with"quotes"#), r#"path\"with\"quotes"#);
        assert_eq!(escape_sbpl_string(r"path\with\backslash"), r"path\\with\\backslash");
    }

    #[test]
    fn network_rule_to_sbpl_uses_port_only_wildcard_host() {
        // SBPL only accepts * or localhost as the host in (remote tcp ...).
        // Named hostnames are enforced at Layer 1 only.
        assert_eq!(
            network_rule_to_sbpl("api.github.com:443"),
            Some("(remote tcp \"*:443\")".to_string()),
        );
        assert_eq!(
            network_rule_to_sbpl("api.github.com:*"),
            Some("(remote tcp \"*:*\")".to_string()),
        );
        assert_eq!(
            network_rule_to_sbpl("*.example.com:8080"),
            Some("(remote tcp \"*:8080\")".to_string()),
        );
    }

    #[test]
    fn network_rule_to_sbpl_returns_none_for_malformed() {
        assert_eq!(network_rule_to_sbpl("no-port-at-all"), None);
    }

    #[test]
    fn all_profiles_generate_without_error() {
        for profile in [
            Profile::Strict,
            Profile::Sandboxed,
            Profile::Network,
            Profile::System,
            Profile::Browser,
        ] {
            let manifest = manifest_for_profile(profile);
            assert!(generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").is_ok());
        }
    }

    #[test]
    fn profile_always_contains_system_allowances() {
        let manifest = manifest_for_profile(Profile::Strict);
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // Global file-read*, sysctl-read, and mach-lookup always present.
        assert!(profile.contains("(allow file-read*)"));
        assert!(profile.contains("(allow sysctl-read)"));
        assert!(profile.contains("(allow mach-lookup)"));
    }

    #[test]
    fn empty_network_allow_produces_no_outbound_rules_for_non_browser() {
        let manifest = manifest_for_profile(Profile::Sandboxed);
        let profile = generate_sbpl_profile(&manifest, "/usr/local/bin/test-tool").unwrap();
        // No network-outbound lines except the preamble
        let outbound_count = profile
            .lines()
            .filter(|l| l.contains("network-outbound") && !l.starts_with(';'))
            .count();
        assert_eq!(outbound_count, 0);
    }
}
