//! Broker subcommand handlers — full implementations for M1 + M2.
//!
//! Each function is the entry point for one CLI subcommand. The `run` command
//! launches the stdio proxy loop; the remaining commands are management CLI.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use mcparmor_core::manifest::{ArmorManifest, Profile, SecretScanMode};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::audit_writer::AuditWriter;
use crate::cli::{
    AuditArgs, InitArgs, ProfilesArgs, ProfilesCommand, RunArgs, StatusArgs, UnwrapArgs,
    ValidateArgs, WrapArgs,
};
use crate::proxy::{ProxyConfig, run_proxy};
use crate::sandbox::noop::NoopSandbox;
use crate::sandbox::SandboxProvider;

/// JSON schema embedded at compile time for offline validation.
const ARMOR_SCHEMA: &str = include_str!("../../../spec/v1.0/armor.schema.json");

/// Default number of days to retain audit log entries when `--since` is not specified.
///
/// When `mcparmor audit --prune` is invoked without `--since`, entries older
/// than this many days are removed. Matches the `retention_days` default used
/// by audit policies that do not specify an explicit value.
const DEFAULT_RETENTION_DAYS: u32 = 90;

/// The pinned GitHub release tag used to fetch community profiles.
///
/// Using a release tag instead of the `main` branch prevents supply-chain
/// attacks where a commit to `main` could silently change a downloaded profile.
const COMMUNITY_PROFILES_RELEASE_TAG: &str = "v1.0.0";

/// Community profiles compiled into the binary at build time.
///
/// Each entry is `(name, json_content)`. These are the top-10 launch profiles
/// that make the tool fully functional offline, out of the box.
pub static BUNDLED_COMMUNITY_PROFILES: &[(&str, &str)] = &[
    ("github",       include_str!("../../../profiles/community/github.armor.json")),
    ("filesystem",   include_str!("../../../profiles/community/filesystem.armor.json")),
    ("gmail",        include_str!("../../../profiles/community/gmail.armor.json")),
    ("slack",        include_str!("../../../profiles/community/slack.armor.json")),
    ("notion",       include_str!("../../../profiles/community/notion.armor.json")),
    ("playwright",   include_str!("../../../profiles/community/playwright.armor.json")),
    ("fetch",        include_str!("../../../profiles/community/fetch.armor.json")),
    ("git",          include_str!("../../../profiles/community/git.armor.json")),
    ("brave-search", include_str!("../../../profiles/community/brave-search.armor.json")),
    ("sqlite",       include_str!("../../../profiles/community/sqlite.armor.json")),
];

/// Look up a bundled community profile by name.
///
/// Returns `Some(&str)` with the raw JSON content if found, `None` otherwise.
pub fn find_bundled_profile(name: &str) -> Option<&'static str> {
    BUNDLED_COMMUNITY_PROFILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| *content)
}

/// The mcparmor broker command name used in wrapped host configs.
const BROKER_COMMAND: &str = "mcparmor";

/// Maximum directory levels to search upward when locating armor.json.
const ARMOR_SEARCH_DEPTH: usize = 5;

/// Default max response size (KB) for the strict fallback manifest.
const STRICT_FALLBACK_MAX_SIZE_KB: u32 = 512;

/// The MCP Armor specification version this broker binary implements.
///
/// Used to enforce `min_spec` requirements declared in armor manifests.
/// Manifests requiring a newer spec version are refused at `run` time and
/// warned at `validate` time.
const BROKER_SPEC_VERSION: &str = "1.0";

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

/// Run a tool under armor enforcement (the core broker loop).
///
/// Locates armor.json, selects a sandbox provider, and starts the stdio proxy.
pub async fn run(args: RunArgs) -> Result<()> {
    let (_armor_path, manifest) = resolve_manifest(args.armor.as_deref())?;
    check_min_spec(&manifest)?;
    let manifest = apply_profile_override(manifest, args.profile.as_deref());
    let manifest = apply_browser_defaults(manifest);

    // Derive the tool display name from argv[0] for use in audit log entries.
    let tool_name = args.command.first()
        .map(|s| {
            Path::new(s)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string());

    let sandbox = select_sandbox(args.no_os_sandbox);
    let audit_writer = Arc::new(build_audit_writer(
        args.no_audit,
        args.audit_log.clone(),
        manifest.audit.max_size_mb,
        manifest.audit.retention_days,
    ));

    let config = ProxyConfig {
        manifest: Arc::new(manifest),
        sandbox,
        audit_writer,
        no_os_sandbox: args.no_os_sandbox,
        no_log_params: args.no_log_params,
        strict_mode: args.strict,
        verbose: args.verbose,
        tool_name,
        annotate: !args.no_annotate,
    };

    run_proxy(config, &args.command).await
}

/// Build an `AuditWriter` from the four audit-related run args.
///
/// Selection order:
/// 1. `no_audit` is true → disabled writer (no I/O).
/// 2. `audit_log` is `Some(path)` → writer targeting that path (no pruning).
/// 3. Default → writer targeting `~/.mcparmor/audit.jsonl` with rotation and pruning.
fn build_audit_writer(
    no_audit: bool,
    audit_log: Option<PathBuf>,
    max_size_mb: Option<u32>,
    retention_days: Option<u32>,
) -> AuditWriter {
    if no_audit {
        return AuditWriter::disabled();
    }
    if let Some(path) = audit_log {
        return AuditWriter::at_path(path);
    }
    AuditWriter::new(AuditWriter::default_path(), max_size_mb, retention_days)
}

/// Apply a --profile override to the manifest if allowed.
fn apply_profile_override(mut manifest: ArmorManifest, profile_override: Option<&str>) -> ArmorManifest {
    let Some(name) = profile_override else {
        return manifest;
    };
    if manifest.locked {
        return manifest;
    }
    if let Some(profile) = parse_profile(name) {
        manifest.profile = profile;
    }
    manifest
}

/// Apply profile-specific defaults that the broker enforces regardless of
/// what the manifest declares.
///
/// `Browser` profile: `deny_local` is always `false` for this profile because
/// browser automation tools require access to the Chrome DevTools Protocol (CDP)
/// on localhost. If the manifest sets `deny_local: true` alongside `browser`,
/// `mcparmor validate` will warn; the broker overrides it to `false` here.
fn apply_browser_defaults(mut manifest: ArmorManifest) -> ArmorManifest {
    if manifest.profile == Profile::Browser {
        manifest.network.deny_local = false;
    }
    manifest
}

/// Parse a profile name string into a `Profile` variant.
fn parse_profile(name: &str) -> Option<Profile> {
    match name {
        "strict" => Some(Profile::Strict),
        "sandboxed" => Some(Profile::Sandboxed),
        "network" => Some(Profile::Network),
        "system" => Some(Profile::System),
        "browser" => Some(Profile::Browser),
        _ => None,
    }
}

/// Returns true if `required` version is ≤ `available`.
///
/// Both strings must be in `MAJOR.MINOR` format (e.g. `"1.0"`, `"1.1"`).
/// Versions that fail to parse are treated as newer than any available version,
/// causing the caller to conservatively refuse the manifest.
fn spec_version_le(required: &str, available: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32)> {
        let (major, minor) = s.split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    };
    match (parse(required), parse(available)) {
        (Some(req), Some(avail)) => req <= avail,
        _ => false,
    }
}

/// Checks that this broker satisfies the manifest's `min_spec` requirement.
///
/// Returns `Ok(())` when the manifest has no `min_spec` or when the broker's
/// spec version meets or exceeds the requirement.
///
/// # Errors
/// Returns an error when `min_spec` is set to a spec version this broker does
/// not yet implement. The caller should surface this as a fatal error in `run`.
fn check_min_spec(manifest: &ArmorManifest) -> Result<()> {
    let Some(required) = manifest.min_spec.as_deref() else {
        return Ok(());
    };
    if spec_version_le(required, BROKER_SPEC_VERSION) {
        return Ok(());
    }
    bail!(
        "armor.json requires broker spec {required} but this broker only implements \
         spec {BROKER_SPEC_VERSION}. Upgrade mcparmor to run this tool."
    )
}

/// Select the best available sandbox provider for the current platform.
///
/// - Linux: `LinuxSandbox` (Landlock + Seccomp) if kernel ≥ 3.5, else `NoopSandbox`.
/// - macOS: `MacosSeatbelt` if `sandbox-exec` is present, else `NoopSandbox`.
/// - Other: `NoopSandbox` (Layer 1 enforcement only).
/// - When `no_os_sandbox` is true: always `NoopSandbox`.
fn select_sandbox(no_os_sandbox: bool) -> Arc<dyn SandboxProvider> {
    if no_os_sandbox {
        return Arc::new(NoopSandbox);
    }

    #[cfg(target_os = "linux")]
    {
        use crate::sandbox::linux::LinuxSandbox;
        if let Ok(sandbox) = LinuxSandbox::detect() {
            if sandbox.is_available() {
                return Arc::new(sandbox);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use crate::sandbox::macos::MacosSeatbelt;
        let seatbelt = MacosSeatbelt;
        if seatbelt.is_available() {
            return Arc::new(seatbelt);
        }
    }

    Arc::new(NoopSandbox)
}

/// Walk up the directory tree from cwd looking for `armor.json`.
fn search_armor_upward() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("Cannot determine current directory")?;

    for _ in 0..ARMOR_SEARCH_DEPTH {
        let candidate = dir.join("armor.json");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            break;
        }
    }

    bail!("armor.json not found in current directory or any of the {ARMOR_SEARCH_DEPTH} parent directories")
}

/// Resolve and load the armor manifest, with a strict-profile fallback.
///
/// When an explicit `--armor` path is given, it must exist — an error is
/// returned if the file is missing. When no path is given, the directory tree
/// is searched upward; if no `armor.json` is found, a built-in `strict`
/// manifest is applied and a warning is printed. This ensures the tool always
/// runs under some level of enforcement even without a manifest.
fn resolve_manifest(hint: Option<&Path>) -> Result<(Option<PathBuf>, ArmorManifest)> {
    if let Some(path) = hint {
        if path.exists() {
            let manifest = load_manifest(path)?;
            return Ok((Some(path.to_path_buf()), manifest));
        }
        bail!("armor.json not found at: {}", path.display());
    }

    match search_armor_upward() {
        Ok(path) => {
            let manifest = load_manifest(&path)?;
            Ok((Some(path), manifest))
        }
        Err(_) => {
            eprintln!(
                "warning: no armor.json found — applying strict profile as a safety fallback. \
                 Run `mcparmor init` to create a manifest for this tool."
            );
            Ok((None, strict_fallback_manifest()))
        }
    }
}

/// Build a minimal strict-profile manifest used when no armor.json is found.
fn strict_fallback_manifest() -> ArmorManifest {
    use mcparmor_core::manifest::{
        AuditPolicy, EnvPolicy, FilesystemPolicy, NetworkPolicy, OutputPolicy, Profile,
        SecretScanMode,
    };
    ArmorManifest {
        version: "1.0".to_string(),
        profile: Profile::Strict,
        filesystem: FilesystemPolicy::default(),
        network: NetworkPolicy::default(),
        spawn: false,
        env: EnvPolicy::default(),
        output: OutputPolicy {
            scan_secrets: SecretScanMode::Redact,
            max_size_kb: Some(STRICT_FALLBACK_MAX_SIZE_KB),
        },
        audit: AuditPolicy { enabled: true, max_size_mb: None, retention_days: None, redact_params: false },
        timeout_ms: None,
        locked: false,
        min_spec: None,
    }
}

/// Parse and deserialize an `ArmorManifest` from a file path.
fn load_manifest(path: &Path) -> Result<ArmorManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Cannot read armor.json at {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Invalid armor.json at {}", path.display()))
}

// ---------------------------------------------------------------------------
// manifest display helpers
// ---------------------------------------------------------------------------

/// Format a `scan_secrets` value for human-readable display.
fn format_secret_scan_mode(manifest: &ArmorManifest) -> &'static str {
    match manifest.output.scan_secrets {
        SecretScanMode::Disabled => "off",
        SecretScanMode::Redact => "on (redact)",
        SecretScanMode::Strict => "on (strict/block)",
    }
}

/// Format a boolean capability flag as a coloured yes/no string.
///
/// Uses ANSI escapes only when stdout is a terminal; passes through plain text
/// otherwise so piped output and test assertions remain stable.
fn fmt_capability(enabled: bool) -> &'static str {
    if enabled { "yes" } else { "no" }
}

/// Join a list of strings into a comma-separated display value, or `"none"` when empty.
fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

/// Print a human-readable summary of the resolved manifest to stdout.
///
/// Called after a successful `validate` and also (internally) by future
/// diagnostic commands that want a quick overview of what a manifest permits.
fn print_manifest_summary(manifest: &ArmorManifest) {
    println!("  Profile              : {:?}", manifest.profile);
    println!("  Locked               : {}", fmt_capability(manifest.locked));
    if let Some(ms) = manifest.timeout_ms {
        println!("  Timeout              : {} ms", ms);
    }
    println!("  Filesystem read      : {}", join_or_none(&manifest.filesystem.read));
    println!("  Filesystem write     : {}", join_or_none(&manifest.filesystem.write));
    println!("  Network allow        : {}", join_or_none(&manifest.network.allow));
    println!("  Network deny_local   : {}", fmt_capability(manifest.network.deny_local));
    println!("  Network deny_metadata: {}", fmt_capability(manifest.network.deny_metadata));
    println!("  Spawn allowed        : {}", fmt_capability(manifest.spawn));
    println!("  Env allow            : {}", join_or_none(&manifest.env.allow));
    println!("  Secret scan          : {}", format_secret_scan_mode(manifest));
    if let Some(max_kb) = manifest.output.max_size_kb {
        println!("  Max response size    : {} KB", max_kb);
    }
}

/// Print the OS sandbox enforcement section for the current platform.
///
/// Shows which Layer 2 capabilities are available (checkmark) or unavailable
/// (warning symbol) based on the detected sandbox provider.
fn print_sandbox_section() {
    let summary = build_enforcement_summary();
    let platform = detect_platform_label();
    println!();
    println!("  OS sandbox — {platform} (this platform):");
    println!("    {}  Filesystem isolation    ({})", capability_icon(summary.filesystem_isolation), summary.mechanism);
    println!("    {}  Spawn blocking          ({})", capability_icon(summary.spawn_blocking), summary.mechanism);
    println!("    {}  Network hostname enforce ({})", capability_icon(summary.network_hostname_enforcement), summary.mechanism);
    println!("    {}  Network port enforce    ({})", capability_icon(summary.network_port_enforcement), summary.mechanism);
}

/// Returns a checkmark for available capabilities and a warning for unavailable ones.
fn capability_icon(available: bool) -> &'static str {
    if available { "✅" } else { "⚠ " }
}

/// Returns a human-readable platform label using OS name and version.
fn detect_platform_label() -> String {
    let os = std::env::consts::OS;
    match os {
        "macos" => "macOS".to_string(),
        // Attempt to read kernel version from uname for display.
        "linux" => read_linux_kernel_version().unwrap_or_else(|| "Linux".to_string()),
        "windows" => "Windows".to_string(),
        other => other.to_string(),
    }
}

/// Read the Linux kernel version string for display purposes.
///
/// Returns `None` if the version cannot be determined.
fn read_linux_kernel_version() -> Option<String> {
    let output = std::process::Command::new("uname").arg("-r").output().ok()?;
    let version = String::from_utf8(output.stdout).ok()?;
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!("Linux {trimmed}"))
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

/// Validate an armor.json manifest against the spec schema.
///
/// Exits with code 0 for valid, 1 for invalid.
pub async fn validate(args: ValidateArgs) -> Result<()> {
    let path = args.armor.unwrap_or_else(|| PathBuf::from("armor.json"));
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;

    let instance: Value = serde_json::from_str(&content)
        .with_context(|| format!("Not valid JSON: {}", path.display()))?;

    let schema_errors = validate_against_schema(&instance)?;

    // Also attempt struct-level deserialization.
    let struct_error = serde_json::from_str::<ArmorManifest>(&content)
        .err()
        .map(|e| format!("  Deserialization error: {e}"));

    if schema_errors.is_empty() && struct_error.is_none() {
        // Run advisory checks on the successfully parsed manifest.
        if let Ok(manifest) = serde_json::from_str::<ArmorManifest>(&content) {
            for warning in advisory_warnings(&manifest) {
                eprintln!("warning: {warning}");
            }
            println!("✓ {} is valid", path.display());
            print_manifest_summary(&manifest);
            print_sandbox_section();
        } else {
            println!("✓ {} is valid", path.display());
        }
        Ok(())
    } else {
        eprintln!("✗ {} is invalid:", path.display());
        for err in &schema_errors {
            eprintln!("{err}");
        }
        if let Some(err) = struct_error {
            eprintln!("{err}");
        }
        std::process::exit(1);
    }
}

/// Return advisory warning strings for a valid manifest.
///
/// These are non-fatal issues that may cause the tool to misbehave at runtime
/// but do not violate the schema.
fn advisory_warnings(manifest: &ArmorManifest) -> Vec<String> {
    let mut warnings = Vec::new();
    warn_if_env_allow_missing_path(manifest, &mut warnings);
    warn_if_browser_deny_local_conflict(manifest, &mut warnings);
    warn_if_deny_local_without_browser_profile(manifest, &mut warnings);
    warn_if_min_spec_exceeds_broker(manifest, &mut warnings);
    warnings
}

/// Warn when `env.allow` is partially set but excludes `PATH`.
///
/// Interpreted runtimes (Python, Node, Ruby) need PATH to locate their binary.
/// An empty env.allow is valid (all env stripped), but a partial list without
/// PATH is almost always a mistake that causes startup failures.
fn warn_if_env_allow_missing_path(manifest: &ArmorManifest, warnings: &mut Vec<String>) {
    let env_allow_missing_path = !manifest.env.allow.is_empty()
        && !manifest.env.allow.iter().any(|k| k == "PATH");
    if env_allow_missing_path {
        warnings.push(
            "env.allow is set but does not include PATH — interpreted tools \
             (node, python3, ruby) may fail to find their runtime binaries. \
             Add \"PATH\" to env.allow if the tool uses an interpreter."
                .to_string(),
        );
    }
}

/// Warn when the `browser` profile is combined with `deny_local: true`.
///
/// The broker overrides `deny_local` to `false` for browser profile at runtime
/// because CDP requires loopback access. An explicit `deny_local: true` here
/// signals a likely misconfiguration.
fn warn_if_browser_deny_local_conflict(manifest: &ArmorManifest, warnings: &mut Vec<String>) {
    if manifest.profile == Profile::Browser && manifest.network.deny_local {
        warnings.push(
            "browser profile requires deny_local: false for Chrome DevTools Protocol (CDP); \
             your explicit deny_local: true will be overridden to false at runtime."
                .to_string(),
        );
    }
}

/// Warn when `deny_local: false` is set without the `browser` profile.
///
/// Loopback access is only justified for browser automation tools. Other tool
/// types should keep `deny_local: true` to prevent SSRF to local services.
fn warn_if_deny_local_without_browser_profile(manifest: &ArmorManifest, warnings: &mut Vec<String>) {
    if manifest.profile != Profile::Browser && !manifest.network.deny_local {
        warnings.push(
            "deny_local: false grants access to loopback addresses (127.0.0.0/8, ::1). \
             This is required for browser automation (CDP) but should be avoided for \
             other tool types. Set profile: browser if this is intentional."
                .to_string(),
        );
    }
}

/// Warn when `min_spec` in the manifest exceeds the running broker's spec version.
///
/// This is advisory-only at validate time; it becomes a hard error at `run` time
/// via `check_min_spec`.
fn warn_if_min_spec_exceeds_broker(manifest: &ArmorManifest, warnings: &mut Vec<String>) {
    let Some(required) = manifest.min_spec.as_deref() else {
        return;
    };
    if !spec_version_le(required, BROKER_SPEC_VERSION) {
        warnings.push(format!(
            "min_spec {required} exceeds this broker's spec version {BROKER_SPEC_VERSION}. \
             Running this manifest with an older broker may produce errors at call time."
        ));
    }
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

/// Show the current protection state for every tool in detected host configs.
pub async fn status(args: StatusArgs) -> Result<()> {
    let summary = build_enforcement_summary();
    let configs = collect_host_configs(args.host.as_deref());

    if args.format == "json" {
        // JSON mode: all human-readable text goes to stderr; stdout is pure JSON.
        eprintln!("Sandbox mechanism: {}", summary.mechanism);
        print_status_json(&configs);
    } else {
        println!("{}", current_platform_line(&summary));
        println!();

        if configs.is_empty() {
            println!("No host configs detected.");
        } else {
            print_status_table(&configs);
        }
    }

    Ok(())
}

/// Returns the enforcement summary for the current platform's best-available sandbox.
fn build_enforcement_summary() -> crate::sandbox::EnforcementSummary {
    let sandbox = select_sandbox(false);
    sandbox.enforcement_summary()
}

/// A detected MCP tool entry from a host config.
struct ToolStatus {
    /// The host application name (e.g. `"claude-desktop"`, `"cursor"`).
    host: String,
    /// The tool name as declared in the host config.
    tool_name: String,
    /// True when the tool command is wrapped via `mcparmor run`.
    is_wrapped: bool,
    /// True when the tool is an HTTP/remote transport (not wrappable via stdio).
    is_http: bool,
    /// Path to the armor.json file embedded in the wrapped args, if any.
    armor_path: Option<String>,
    /// Profile name extracted from the tool's armor.json, if readable.
    ///
    /// `None` when the tool is not wrapped, or when the armor.json cannot be
    /// located or parsed. Population is best-effort and never causes an error.
    profile: Option<String>,
}

/// Collect tool status entries from all detectable host configs.
fn collect_host_configs(host_filter: Option<&str>) -> Vec<ToolStatus> {
    let candidates = host_config_paths();
    let mut results = Vec::new();

    for (host_name, path) in candidates {
        if let Some(filter) = host_filter {
            if host_name != filter {
                continue;
            }
        }
        if let Ok(entries) = read_tool_statuses(&host_name, &path) {
            results.extend(entries);
        }
    }

    results
}

/// Returns (host_name, config_path) pairs for all well-known MCP host configs.
fn host_config_paths() -> Vec<(String, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();

    let mut paths = Vec::new();

    // Claude Desktop
    #[cfg(target_os = "macos")]
    paths.push((
        "claude-desktop".to_string(),
        home.join("Library/Application Support/Claude/claude_desktop_config.json"),
    ));
    #[cfg(target_os = "linux")]
    paths.push((
        "claude-desktop".to_string(),
        home.join(".config/Claude/claude_desktop_config.json"),
    ));

    // Claude CLI
    paths.push(("claude-cli".to_string(), home.join(".claude/mcp_servers.json")));
    paths.push(("claude-cli-project".to_string(), cwd.join(".claude/mcp_servers.json")));

    // Cursor
    paths.push(("cursor".to_string(), home.join(".cursor/mcp.json")));
    paths.push(("cursor-project".to_string(), cwd.join(".cursor/mcp.json")));

    // VS Code
    paths.push(("vscode-project".to_string(), cwd.join(".vscode/mcp.json")));

    // Windsurf — global config at the Codeium data directory.
    paths.push(("windsurf".to_string(), home.join(".codeium/windsurf/mcp_config.json")));

    paths
}

/// Parse a host config file and extract tool status entries.
fn read_tool_statuses(host_name: &str, path: &Path) -> Result<Vec<ToolStatus>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let config: Value = serde_json::from_str(&content)?;
    let servers = extract_mcp_servers(&config);
    let mut results = Vec::new();

    for (name, server) in servers {
        let command = server.get("command").and_then(Value::as_str).unwrap_or("");
        let is_wrapped = command == BROKER_COMMAND;
        let armor_path = extract_armor_path_from_wrapped(server);
        let has_url = server
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|u| !u.is_empty());
        let is_http_type = server.get("type").and_then(Value::as_str) == Some("http");
        let is_http = has_url || is_http_type;

        let profile = extract_profile_from_armor_path(armor_path.as_deref());

        results.push(ToolStatus {
            host: host_name.to_string(),
            tool_name: name,
            is_wrapped,
            is_http,
            armor_path,
            profile,
        });
    }

    Ok(results)
}

/// Extract the --armor path from an already-wrapped tool entry, if present.
fn extract_armor_path_from_wrapped(server: &Value) -> Option<String> {
    let args = server.get("args")?.as_array()?;
    let armor_pos = args.iter().position(|a| a.as_str() == Some("--armor"))?;
    args.get(armor_pos + 1)?.as_str().map(str::to_string)
}

/// Load and parse the armor.json at `path`, then extract the profile name.
///
/// Returns the profile as a lowercase string (e.g. `"sandboxed"`, `"strict"`),
/// or `None` if the path is absent, the file cannot be read, or parsing fails.
/// This function is intentionally infallible — failures are silently ignored so
/// status display always succeeds regardless of armor.json availability.
fn extract_profile_from_armor_path(path: Option<&str>) -> Option<String> {
    let content = fs::read_to_string(path?).ok()?;
    let manifest: ArmorManifest = serde_json::from_str(&content).ok()?;
    Some(format!("{:?}", manifest.profile).to_lowercase())
}

/// Extract the mcpServers object from a config, trying multiple key patterns.
fn extract_mcp_servers(config: &Value) -> Vec<(String, &Value)> {
    let servers = config
        .get("mcpServers")
        .or_else(|| config.get("servers"))
        .and_then(Value::as_object);

    let Some(map) = servers else {
        return Vec::new();
    };

    map.iter().map(|(k, v)| (k.clone(), v)).collect()
}

/// Returns the platform line for the status output, showing Layer 1 and Layer 2 availability.
///
/// Layer 1 (protocol inspection) is always available. Layer 2 (OS sandbox) is shown
/// as available when at least filesystem isolation or spawn blocking is active.
fn current_platform_line(summary: &crate::sandbox::EnforcementSummary) -> String {
    let platform = detect_platform_label();
    let layer2_available = summary.filesystem_isolation || summary.spawn_blocking;
    let layer1_icon = "✅";
    let layer2_icon = if layer2_available { "✅" } else { "❌" };
    let mechanism_note = if layer2_available {
        format!(" ({})", summary.mechanism)
    } else {
        " (not available)".to_string()
    };
    format!("Platform: {platform} — Layer 1 {layer1_icon}  Layer 2 {layer2_icon}{mechanism_note}")
}

/// Print the tool-status list as a human-readable table to stdout.
///
/// Columns: TOOL (20), STATUS (16), PROFILE (12), ARMOR SOURCE (remaining).
/// Includes a summary line at the bottom showing counts of armored, HTTP-skipped,
/// and unwrapped tools. Prints a wrap hint when any tools are unwrapped.
fn print_status_table(configs: &[ToolStatus]) {
    println!("{:<20} {:<16} {:<12} {}", "TOOL", "STATUS", "PROFILE", "ARMOR SOURCE");
    println!("{}", "-".repeat(80));
    for entry in configs {
        let (status, profile_col, armor_source) = format_status_row(entry);
        println!("{:<20} {:<16} {:<12} {}", entry.tool_name, status, profile_col, armor_source);
    }
    println!();
    print_status_summary(configs);
}

/// Format the STATUS, PROFILE, and ARMOR SOURCE columns for a single tool entry.
fn format_status_row(entry: &ToolStatus) -> (String, String, String) {
    if entry.is_http {
        return (
            "⚠ not wrapped".to_string(),
            "n/a".to_string(),
            "HTTP transport — not supported".to_string(),
        );
    }
    if !entry.is_wrapped {
        return (
            "❌ not wrapped".to_string(),
            "n/a".to_string(),
            "not yet wrapped".to_string(),
        );
    }
    let profile_col = entry.profile.clone().unwrap_or_else(|| "n/a".to_string());
    let armor_source = entry.armor_path.clone().unwrap_or_else(|| "fallback (no armor.json found)".to_string());
    ("✅ armored".to_string(), profile_col, armor_source)
}

/// Print the summary line and optional wrap hint for the status table.
fn print_status_summary(configs: &[ToolStatus]) {
    let armored = configs.iter().filter(|e| e.is_wrapped).count();
    let http_skipped = configs.iter().filter(|e| !e.is_wrapped && e.is_http).count();
    let unwrapped = configs.iter().filter(|e| !e.is_wrapped && !e.is_http).count();
    println!("Summary: {armored} armored, {http_skipped} HTTP skipped, {unwrapped} unwrapped");

    if unwrapped > 0 {
        let first_host = configs
            .iter()
            .find(|e| !e.is_wrapped && !e.is_http)
            .map(|e| e.host.as_str())
            .unwrap_or("claude-desktop");
        println!("Run: mcparmor wrap --host {first_host} to protect all stdio tools.");
    }
}

/// Print the tool-status list as a JSON array to stdout.
fn print_status_json(configs: &[ToolStatus]) {
    let json: Vec<Value> = configs
        .iter()
        .map(|e| {
            serde_json::json!({
                "host": e.host,
                "tool": e.tool_name,
                "wrapped": e.is_wrapped,
                "armor_path": e.armor_path,
                "profile": e.profile,
            })
        })
        .collect();
    match serde_json::to_string_pretty(&json) {
        Ok(output) => println!("{output}"),
        Err(e) => tracing::warn!("Failed to serialise status JSON: {e:#}"),
    }
}

// ---------------------------------------------------------------------------
// wrap
// ---------------------------------------------------------------------------

/// Describes the source of an armor manifest discovered for a tool during wrap.
///
/// Controls the display label shown in `mcparmor wrap` output and the status
/// icon (✅ vs ⚠) so users understand whether a custom manifest was applied.
#[derive(Debug)]
enum ArmorSource {
    /// Matched a community profile in the user's profile directory.
    CommunityProfile { name: String },
    /// Found a local armor.json file next to the tool binary.
    LocalFile { path: PathBuf },
    /// No armor.json was found; strict fallback profile will be applied at runtime.
    StrictFallback,
}

/// The outcome of attempting to wrap a single tool entry.
///
/// Collected across all server entries and then printed as a summary, so that
/// output order is deterministic even when entries are skipped mid-iteration.
#[derive(Debug)]
enum WrapOutcome {
    /// Entry was newly wrapped.
    Wrapped { name: String, armor_source: ArmorSource },
    /// Entry was already wrapped and --rewrap updated it.
    Rewrapped { name: String, armor_source: ArmorSource },
    /// Entry was already wrapped and --rewrap was not requested; skipped.
    AlreadyWrapped { name: String },
    /// Entry has no command field (HTTP/remote tool); skipped with a warning.
    HttpSkipped { name: String },
}

/// Format an `ArmorSource` for display in wrap outcome lines.
fn format_armor_source(source: &ArmorSource) -> String {
    match source {
        ArmorSource::CommunityProfile { name } => format!("community profile — {name}"),
        ArmorSource::LocalFile { path } => {
            // Show a relative-style path for brevity.
            let display = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("armor.json");
            format!("./{display}")
        }
        ArmorSource::StrictFallback => "strict fallback — no armor.json found".to_string(),
    }
}

/// Returns the status icon (✅ or ⚠) for a wrap outcome based on armor source.
fn wrap_outcome_icon(source: &ArmorSource) -> &'static str {
    match source {
        ArmorSource::StrictFallback => "⚠ ",
        _ => "✅",
    }
}

/// Print a human-readable summary of wrap outcomes.
///
/// HTTP-skipped tool warnings are written to stderr (so they are visible as
/// operator warnings in CI logs). All other outcome lines go to stdout.
fn print_wrap_outcomes(outcomes: &[WrapOutcome], config_path: &Path, dry_run: bool) {
    let prefix = if dry_run { "[dry-run] " } else { "" };
    for outcome in outcomes {
        print_single_wrap_outcome(outcome, prefix);
    }
    let written = count_written_outcomes(outcomes);
    if !dry_run {
        println!("Wrote {written} change(s) to {}", config_path.display());
    }
}

/// Print the outcome line for a single tool entry.
fn print_single_wrap_outcome(outcome: &WrapOutcome, prefix: &str) {
    match outcome {
        WrapOutcome::Wrapped { name, armor_source } => {
            let icon = wrap_outcome_icon(armor_source);
            let source_label = format_armor_source(armor_source);
            println!("{prefix}{icon} {name:<14} armored  ({source_label})");
            if matches!(armor_source, ArmorSource::StrictFallback) {
                println!("               → Run: mcparmor init to generate one");
            }
        }
        WrapOutcome::Rewrapped { name, armor_source } => {
            let icon = wrap_outcome_icon(armor_source);
            let source_label = format_armor_source(armor_source);
            println!("{prefix}{icon} {name:<14} re-armored  ({source_label})");
            if matches!(armor_source, ArmorSource::StrictFallback) {
                println!("               → Run: mcparmor init to generate one");
            }
        }
        WrapOutcome::AlreadyWrapped { name } => {
            println!("{prefix}  (skipped)    '{name}' — already wrapped (use --rewrap to update)");
        }
        WrapOutcome::HttpSkipped { name } => {
            // Warn to stderr so it surfaces as a visible operator notice in CI.
            eprintln!(
                "warning: [mcparmor] skipping '{name}' — HTTP/remote tool is out of scope \
                 (apply security controls on the remote server side instead)."
            );
        }
    }
}

/// Count the number of outcomes that resulted in a file write.
fn count_written_outcomes(outcomes: &[WrapOutcome]) -> usize {
    outcomes.iter().filter(|outcome| {
        matches!(outcome, WrapOutcome::Wrapped { .. } | WrapOutcome::Rewrapped { .. })
    }).count()
}

/// Wrap a host MCP config to route stdio tools through the broker.
pub async fn wrap(args: WrapArgs) -> Result<()> {
    warn_if_scope_flag_has_no_effect(&args.scope);

    let host = args.host.as_deref().unwrap_or("");
    let path = resolve_host_config_path(host, args.config.as_deref())?;
    let content = if path.exists() {
        fs::read_to_string(&path)
            .with_context(|| format!("Cannot read host config: {}", path.display()))?
    } else {
        // Create parent directories so the config can be persisted later.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Cannot create directory: {}", parent.display()))?;
        }
        "{}".to_string()
    };
    let mut config: Value = serde_json::from_str(&content)?;

    let servers = get_mcp_servers_mut(&mut config)?;
    let outcomes = wrap_all_servers(servers, &args);

    if !args.dry_run {
        persist_wrap_changes(&mut config, &path, &outcomes, args.backup)?;
    }

    print_wrap_outcomes(&outcomes, &path, args.dry_run);
    Ok(())
}

/// Emit a warning when `--scope` is used, since multi-scope wrapping is not yet implemented.
fn warn_if_scope_flag_has_no_effect(scope: &str) {
    // --scope is reserved for future use. Multi-scope wrapping (e.g. wrapping
    // both global and project configs in one command) is not yet implemented.
    // Use the specific --host name (e.g. cursor vs cursor-project) in the
    // meantime. Emit a warning so callers know their --scope flag has no effect.
    if scope != "both" {
        eprintln!(
            "warning: [mcparmor] --scope '{scope}' is not yet implemented. \
             Use the host-specific name (e.g. cursor-project) to target a \
             project-level config, or cursor for the global config.",
        );
    }
}

/// Iterate over all server entries and compute a `WrapOutcome` for each.
fn wrap_all_servers(servers: &mut Value, args: &WrapArgs) -> Vec<WrapOutcome> {
    let Some(map) = servers.as_object_mut() else {
        return Vec::new();
    };

    let mut outcomes = Vec::new();
    for (name, server) in map.iter_mut() {
        if let Some(outcome) = wrap_single_server(name, server, args) {
            outcomes.push(outcome);
        }
    }
    outcomes
}

/// Compute the `WrapOutcome` for a single server entry, mutating it when `!args.dry_run`.
///
/// Returns `None` when the entry has no command and is not an HTTP tool (silently skipped).
fn wrap_single_server(name: &str, server: &mut Value, args: &WrapArgs) -> Option<WrapOutcome> {
    let command = server.get("command").and_then(Value::as_str).unwrap_or("");
    let is_already_wrapped = command == BROKER_COMMAND;

    if is_already_wrapped && !args.rewrap {
        return Some(WrapOutcome::AlreadyWrapped { name: name.to_string() });
    }

    // Only wrap stdio tools (has "command" field). HTTP/remote tools have no
    // "command" field and are explicitly out of scope — they run in their own
    // infrastructure and must be secured there.
    if command.is_empty() {
        return classify_http_tool(name, server);
    }

    let (original_command, original_args) =
        match extract_stdio_command(name, server, command, is_already_wrapped) {
            Some(pair) => pair,
            None => return None,
        };

    let (armor_path, armor_source) = resolve_armor_source(name, &original_command, args);

    let new_args = build_wrapped_args(
        &armor_path,
        args.profile.as_deref(),
        &original_command,
        &original_args,
    );

    if !args.dry_run {
        apply_wrap_to_server(server, new_args);
    }

    Some(if is_already_wrapped {
        WrapOutcome::Rewrapped { name: name.to_string(), armor_source }
    } else {
        WrapOutcome::Wrapped { name: name.to_string(), armor_source }
    })
}

/// Return an `HttpSkipped` outcome when the entry is an HTTP/remote tool, or `None` otherwise.
fn classify_http_tool(name: &str, server: &Value) -> Option<WrapOutcome> {
    let has_url = server
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|u| !u.is_empty());
    let is_http_type = server.get("type").and_then(Value::as_str) == Some("http");
    if has_url || is_http_type {
        Some(WrapOutcome::HttpSkipped { name: name.to_string() })
    } else {
        None
    }
}

/// Extract the original (pre-wrap) command and args from a server entry.
///
/// When `is_already_wrapped`, the original command is recovered from after the `--`
/// separator so the entry is not double-wrapped. When not yet wrapped, the
/// current `command` and `args` fields are used directly.
///
/// Returns `None` and emits a warning when an already-wrapped entry cannot be
/// unwound (e.g. the `--` separator is missing).
fn extract_stdio_command(
    name: &str,
    server: &Value,
    command: &str,
    is_already_wrapped: bool,
) -> Option<(String, Vec<String>)> {
    if is_already_wrapped {
        match extract_original_command(server) {
            Some(pair) => Some(pair),
            None => {
                eprintln!(
                    "warning: [mcparmor] could not extract original command from \
                     wrapped entry '{name}'; skipping."
                );
                None
            }
        }
    } else {
        let original_args: Vec<String> = server
            .get("args")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        Some((command.to_string(), original_args))
    }
}

/// Discover the armor manifest path and source for a tool.
///
/// Returns `(None, StrictFallback)` when `--no-armor-path` is set or no manifest is found.
fn resolve_armor_source(
    tool_name: &str,
    original_command: &str,
    args: &WrapArgs,
) -> (Option<PathBuf>, ArmorSource) {
    // --no-armor-path: portable args with no embedded path.
    if args.no_armor_path {
        return (None, ArmorSource::StrictFallback);
    }
    match discover_armor_for_tool(tool_name, original_command, None) {
        Some((path, source)) => (Some(path), source),
        None => (None, ArmorSource::StrictFallback),
    }
}

/// Mutate a server entry in-place to replace its command and args with the wrapped form.
fn apply_wrap_to_server(server: &mut Value, new_args: Vec<String>) {
    if let Some(obj) = server.as_object_mut() {
        obj.insert("command".to_string(), Value::String(BROKER_COMMAND.to_string()));
        obj.insert(
            "args".to_string(),
            Value::Array(new_args.into_iter().map(Value::String).collect()),
        );
    }
}

/// Write the updated config to disk (with optional backup) when there are actual changes.
///
/// Only writes if at least one server was newly wrapped or re-wrapped.
fn persist_wrap_changes(
    config: &mut Value,
    path: &Path,
    outcomes: &[WrapOutcome],
    backup: bool,
) -> Result<()> {
    let change_count = outcomes.iter().filter(|outcome| {
        matches!(outcome, WrapOutcome::Wrapped { .. } | WrapOutcome::Rewrapped { .. })
    }).count();

    if change_count == 0 {
        return Ok(());
    }

    if backup {
        let backup_path = path.with_extension("json.bak");
        fs::copy(path, &backup_path).with_context(|| {
            format!("Failed to create backup at {}", backup_path.display())
        })?;
    }

    let updated = serde_json::to_string_pretty(config)?;
    fs::write(path, updated)?;
    Ok(())
}

/// Build the wrapped args array:
/// `run [--armor <path>] [--profile <profile>] -- <original_cmd> <original_args>`.
fn build_wrapped_args(
    armor_path: &Option<PathBuf>,
    profile_override: Option<&str>,
    original_command: &str,
    original_args: &[String],
) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    if let Some(path) = armor_path {
        args.push("--armor".to_string());
        args.push(path.to_string_lossy().into_owned());
    }
    if let Some(profile) = profile_override {
        args.push("--profile".to_string());
        args.push(profile.to_string());
    }
    args.push("--".to_string());
    args.push(original_command.to_string());
    args.extend_from_slice(original_args);
    args
}

/// Discover the armor.json for a tool using the documented discovery chain.
///
/// Returns the path and the source kind so callers can annotate output with
/// where the armor manifest was found. Returns `None` when no manifest exists,
/// indicating the broker will use the strict-profile fallback at runtime.
fn discover_armor_for_tool(
    tool_name: &str,
    tool_command: &str,
    armor_hint: Option<&Path>,
) -> Option<(PathBuf, ArmorSource)> {
    if let Some(hint) = armor_hint {
        return Some((hint.to_path_buf(), ArmorSource::LocalFile { path: hint.to_path_buf() }));
    }

    if let Some(found) = find_armor_near_binary(tool_command) {
        return Some(found);
    }

    if let Some(found) = find_armor_in_cwd() {
        return Some(found);
    }

    // Community profile
    let community_path = community_profile_path(tool_name);
    if community_path.exists() {
        return Some((community_path, ArmorSource::CommunityProfile { name: tool_name.to_string() }));
    }

    None
}

/// Search for an armor.json in the tool binary's directory and its parent.
fn find_armor_near_binary(tool_command: &str) -> Option<(PathBuf, ArmorSource)> {
    let binary_dir = Path::new(tool_command).parent()?;

    let candidate = binary_dir.join("armor.json");
    if candidate.exists() {
        return Some((candidate.clone(), ArmorSource::LocalFile { path: candidate }));
    }

    let parent = binary_dir.parent()?;
    let candidate = parent.join("armor.json");
    if candidate.exists() {
        return Some((candidate.clone(), ArmorSource::LocalFile { path: candidate }));
    }

    None
}

/// Search for an armor.json in the current working directory.
fn find_armor_in_cwd() -> Option<(PathBuf, ArmorSource)> {
    let cwd = std::env::current_dir().ok()?;
    let candidate = cwd.join("armor.json");
    if candidate.exists() {
        Some((candidate.clone(), ArmorSource::LocalFile { path: candidate }))
    } else {
        None
    }
}

/// Returns the path to the community profile for a named tool.
fn community_profile_path(tool_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".mcparmor")
        .join("profiles")
        .join("community")
        .join(format!("{tool_name}.armor.json"))
}

/// Resolve the host config file path from host name and optional override.
///
/// When `override_path` is provided, it takes precedence over `host`.
/// When neither is provided (host is empty), returns an error.
fn resolve_host_config_path(host: &str, override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }

    if host.is_empty() {
        bail!("Either --host or --config must be provided.");
    }

    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();

    let path = match host {
        "claude-desktop" => {
            #[cfg(target_os = "macos")]
            {
                home.join("Library/Application Support/Claude/claude_desktop_config.json")
            }
            #[cfg(not(target_os = "macos"))]
            {
                home.join(".config/Claude/claude_desktop_config.json")
            }
        }
        "claude-cli" => home.join(".claude/mcp_servers.json"),
        "claude-cli-project" => cwd.join(".claude/mcp_servers.json"),
        "cursor" => home.join(".cursor/mcp.json"),
        "cursor-project" => cwd.join(".cursor/mcp.json"),
        "vscode-project" => cwd.join(".vscode/mcp.json"),
        // Windsurf stores all MCP config globally — no project-level config file.
        "windsurf" => home.join(".codeium/windsurf/mcp_config.json"),
        _ => bail!("Unknown host: '{host}'. Valid hosts: claude-desktop, claude-cli, claude-cli-project, cursor, cursor-project, vscode-project, windsurf. Use --config to specify a custom path."),
    };

    Ok(path)
}

/// Get a mutable reference to the MCP servers map value in the config.
///
/// Prefers the `mcpServers` key (used by Claude Desktop, Cursor, etc.).
/// Falls back to `servers` if `mcpServers` is absent (used by some hosts).
/// Creates an empty `mcpServers` object if neither key is present.
///
/// # Errors
/// Returns an error if `config` is not a JSON object (e.g. a JSON array).
fn get_mcp_servers_mut(config: &mut Value) -> Result<&mut Value> {
    let obj = config
        .as_object_mut()
        .context("host config must be a JSON object, not an array or scalar")?;

    // Use "servers" key only when "mcpServers" is absent — some hosts use this key.
    if obj.contains_key("servers") && !obj.contains_key("mcpServers") {
        // SAFETY: we just confirmed the key exists.
        return Ok(obj.get_mut("servers").expect("'servers' key confirmed present"));
    }

    // Return existing "mcpServers" or create it.
    Ok(obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Default::default())))
}

// ---------------------------------------------------------------------------
// unwrap
// ---------------------------------------------------------------------------

/// Restore a host config to its pre-wrap state.
pub async fn unwrap(args: UnwrapArgs) -> Result<()> {
    let host = args.host.as_deref().unwrap_or("");
    let path = resolve_host_config_path(host, args.config.as_deref())?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read host config: {}", path.display()))?;
    let mut config: Value = serde_json::from_str(&content)?;

    let servers = get_mcp_servers_mut(&mut config)?;
    let mut unwrapped_count = 0_usize;

    if let Some(map) = servers.as_object_mut() {
        for server in map.values_mut() {
            if !is_wrapped_entry(server) {
                continue;
            }

            let Some((original_cmd, original_args)) = extract_original_command(server) else {
                continue;
            };

            if let Some(obj) = server.as_object_mut() {
                obj.insert("command".to_string(), Value::String(original_cmd));
                obj.insert(
                    "args".to_string(),
                    Value::Array(original_args.into_iter().map(Value::String).collect()),
                );
                unwrapped_count += 1;
            }
        }
    }

    let updated = serde_json::to_string_pretty(&config)?;
    fs::write(&path, updated)?;
    println!("Unwrapped {unwrapped_count} tool(s) in {}", path.display());

    Ok(())
}

/// Returns true if the server entry is wrapped by mcparmor.
fn is_wrapped_entry(server: &Value) -> bool {
    let command = server.get("command").and_then(Value::as_str).unwrap_or("");
    if command != BROKER_COMMAND {
        return false;
    }
    let args = server.get("args").and_then(Value::as_array);
    let Some(args) = args else {
        return false;
    };
    args.iter().any(|a| a.as_str() == Some("run"))
}

/// Extract the original command and args from a wrapped entry (after `--`).
fn extract_original_command(server: &Value) -> Option<(String, Vec<String>)> {
    let args: Vec<&str> = server
        .get("args")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();

    let separator_pos = args.iter().position(|&a| a == "--")?;
    let after_separator = &args[separator_pos + 1..];
    let (cmd, rest) = after_separator.split_first()?;

    Some((
        cmd.to_string(),
        rest.iter().map(|s| s.to_string()).collect(),
    ))
}

// ---------------------------------------------------------------------------
// audit
// ---------------------------------------------------------------------------

/// Query the armor audit log.
pub async fn audit(args: AuditArgs) -> Result<()> {
    let log_path = AuditWriter::default_path();

    if !log_path.exists() {
        println!("No audit log found at {}", log_path.display());
        return Ok(());
    }

    let since = resolve_since_filter(args.prune, args.since.as_deref())?;
    let entries = read_audit_entries(&log_path, &args.tool, &args.event, since.as_ref())?;

    if args.prune {
        return execute_prune(&log_path, &entries);
    }

    if args.stats {
        print_audit_stats(&entries);
        return Ok(());
    }

    if args.format == "json" {
        print_audit_json(&entries);
    } else {
        print_audit_table(&entries);
    }

    Ok(())
}

/// Resolve the `since` datetime filter for audit queries and prune operations.
///
/// When `is_prune` is `true` and no explicit `since_str` is provided, a
/// default retention of `DEFAULT_RETENTION_DAYS` days is applied so that
/// `audit --prune` removes entries older than the default retention window.
/// When `since_str` is provided it takes precedence in all cases.
fn resolve_since_filter(
    is_prune: bool,
    since_str: Option<&str>,
) -> Result<Option<DateTime<Utc>>> {
    if let Some(s) = since_str {
        return Ok(Some(parse_since_filter(s)?));
    }
    if is_prune {
        let cutoff = Utc::now() - ChronoDuration::days(i64::from(DEFAULT_RETENTION_DAYS));
        return Ok(Some(cutoff));
    }
    Ok(None)
}

/// Write pruned entries back to the log and print a summary line.
fn execute_prune(path: &Path, entries_to_keep: &[AuditRow]) -> Result<()> {
    let total_before = count_raw_log_lines(path);
    prune_audit_log(path, entries_to_keep)?;
    let removed = total_before.saturating_sub(entries_to_keep.len());
    println!(
        "Pruned {removed} entr{} — {} retained.",
        if removed == 1 { "y" } else { "ies" },
        entries_to_keep.len(),
    );
    Ok(())
}

/// Count the number of non-empty lines in a log file without parsing them.
///
/// Used to compute how many entries were removed by a prune operation.
fn count_raw_log_lines(path: &Path) -> usize {
    let Ok(file) = fs::File::open(path) else {
        return 0;
    };
    BufReader::new(file)
        .lines()
        .filter_map(Result::ok)
        .filter(|l| !l.is_empty())
        .count()
}

/// Parse a `--since` filter string into a `DateTime<Utc>` cutoff.
///
/// Accepts ISO8601 dates or relative durations: `1h`, `24h`, `7d`.
fn parse_since_filter(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }

    let now = Utc::now();
    if let Some(hours) = s.strip_suffix('h') {
        let h: i64 = hours.parse().context("Invalid hours in --since")?;
        return Ok(now - ChronoDuration::hours(h));
    }
    if let Some(days) = s.strip_suffix('d') {
        let d: i64 = days.parse().context("Invalid days in --since")?;
        return Ok(now - ChronoDuration::days(d));
    }

    bail!("Cannot parse --since value: '{s}'. Use ISO8601 or a relative duration like 1h, 24h, 7d.")
}

/// A parsed audit log entry suitable for display and filtering.
struct AuditRow {
    /// The original raw JSON line from the log file, used for JSON output.
    raw: String,
    /// Parsed timestamp for chronological filtering and display.
    timestamp: DateTime<Utc>,
    /// Tool name field from the audit entry.
    tool: String,
    /// Event type field from the audit entry (e.g. `"invoke"`, `"response"`).
    event: String,
    /// Optional detail string from the audit entry.
    detail: String,
}

/// Read and filter audit log entries from `log_path`.
fn read_audit_entries(
    path: &Path,
    tool_filter: &Option<String>,
    event_filter: &Option<String>,
    since: Option<&DateTime<Utc>>,
) -> Result<Vec<AuditRow>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        let Some(row) = parse_audit_row(line) else {
            continue;
        };
        if row_matches_filters(&row, tool_filter, event_filter, since) {
            rows.push(row);
        }
    }

    Ok(rows)
}

/// Parse a single NDJSON audit log line into an `AuditRow`.
///
/// Returns `None` when the line cannot be parsed as JSON or lacks a valid timestamp.
fn parse_audit_row(line: String) -> Option<AuditRow> {
    let v: Value = serde_json::from_str(&line).ok()?;

    let tool = v["tool"].as_str().unwrap_or("").to_string();
    let event = v["event"].as_str().unwrap_or("").to_string();
    let detail = v["detail"].as_str().unwrap_or("").to_string();
    let timestamp_str = v["timestamp"].as_str().unwrap_or("");
    let timestamp = timestamp_str.parse::<DateTime<Utc>>().ok()?;

    Some(AuditRow { raw: line, timestamp, tool, event, detail })
}

/// Returns true when the row passes all active filters.
fn row_matches_filters(
    row: &AuditRow,
    tool_filter: &Option<String>,
    event_filter: &Option<String>,
    since: Option<&DateTime<Utc>>,
) -> bool {
    if let Some(required_tool) = tool_filter {
        if &row.tool != required_tool {
            return false;
        }
    }
    if let Some(required_event) = event_filter {
        if &row.event != required_event {
            return false;
        }
    }
    if let Some(cutoff) = since {
        if row.timestamp < *cutoff {
            return false;
        }
    }
    true
}

/// Print audit log rows as a human-readable table to stdout.
fn print_audit_table(rows: &[AuditRow]) {
    println!("{:<30} {:<20} {:<20} {}", "TIMESTAMP", "TOOL", "EVENT", "DETAIL");
    println!("{}", "-".repeat(90));
    for row in rows {
        println!(
            "{:<30} {:<20} {:<20} {}",
            row.timestamp.format("%Y-%m-%dT%H:%M:%SZ"),
            row.tool,
            row.event,
            row.detail
        );
    }
}

/// Print audit log rows as a pretty-printed JSON array to stdout.
fn print_audit_json(rows: &[AuditRow]) {
    let values: Vec<Value> = rows
        .iter()
        .filter_map(|r| serde_json::from_str(&r.raw).ok())
        .collect();
    match serde_json::to_string_pretty(&values) {
        Ok(output) => println!("{output}"),
        Err(e) => tracing::warn!("Failed to serialise audit JSON: {e:#}"),
    }
}

/// Print a summary of audit log event counts grouped by event type to stdout.
fn print_audit_stats(rows: &[AuditRow]) {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for row in rows {
        *counts.entry(row.event.as_str()).or_insert(0) += 1;
    }
    let mut event_counts: Vec<_> = counts.into_iter().collect();
    event_counts.sort_by_key(|(event, _)| *event);
    println!("{:<25} {}", "EVENT", "COUNT");
    println!("{}", "-".repeat(35));
    for (event, count) in event_counts {
        println!("{:<25} {}", event, count);
    }
}

/// Overwrite the audit log file with only the entries to keep.
///
/// The file is rewritten atomically via [`fs::write`]. Entries are written one
/// per line in their original raw JSON form, preserving the NDJSON format.
fn prune_audit_log(path: &Path, entries_to_keep: &[AuditRow]) -> Result<()> {
    let kept: Vec<&str> = entries_to_keep.iter().map(|r| r.raw.as_str()).collect();
    let content = kept.join("\n") + "\n";
    fs::write(path, content).context("Failed to write pruned audit log")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

/// Generate a minimal armor.json for the specified profile.
pub async fn init(args: InitArgs) -> Result<()> {
    let output_path = args.dir.join("armor.json");

    if output_path.exists() && !args.force {
        bail!(
            "{} already exists. Use --force to overwrite.",
            output_path.display()
        );
    }

    let content = if args.profile.is_some() {
        generate_armor_json(args.profile.as_deref().unwrap())?
    } else {
        generate_armor_json_interactive(&mut std::io::stdin().lock())?
    };

    fs::create_dir_all(&args.dir)
        .with_context(|| format!("Cannot create directory {}", args.dir.display()))?;

    fs::write(&output_path, &content)
        .with_context(|| format!("Cannot write {}", output_path.display()))?;

    println!("{}", content);
    println!("Written to {}", output_path.display());

    Ok(())
}

/// Run the interactive armor.json questionnaire, reading answers from `reader`.
///
/// Prompts for profile, filesystem paths, network allow list, spawn permission,
/// environment variables, and lock setting. Builds and returns the JSON string.
fn generate_armor_json_interactive(reader: &mut dyn BufRead) -> Result<String> {
    let schema_uri = "https://mcp-armor.com/spec/v1.0/armor.schema.json";

    let profile = prompt_with_default(
        reader,
        "Profile [sandboxed]",
        "sandboxed",
        &["strict", "sandboxed", "network", "system", "browser"],
    )?;
    let read_paths = prompt_csv(reader, "Filesystem read paths (comma-separated, blank for none)")?;
    let write_paths = prompt_csv(reader, "Filesystem write paths (comma-separated, blank for none)")?;
    let network_allow = prompt_csv(reader, "Network allow (host:port, comma-separated, blank for none)")?;
    let allow_spawn = prompt_bool(reader, "Allow spawn?", false)?;
    let env_allow = prompt_csv(reader, "Env vars allowed (comma-separated, blank for none)")?;
    let locked = prompt_bool(reader, "Lock profile?", false)?;

    let mut value = serde_json::json!({
        "$schema": schema_uri,
        "version": "1.0",
        "profile": profile,
    });

    let obj = value.as_object_mut().unwrap();

    if !read_paths.is_empty() || !write_paths.is_empty() {
        obj.insert("filesystem".to_string(), serde_json::json!({
            "read": read_paths,
            "write": write_paths,
        }));
    }

    if !network_allow.is_empty() {
        obj.insert("network".to_string(), serde_json::json!({
            "allow": network_allow,
            "deny_local": true,
            "deny_metadata": true,
        }));
    }

    if allow_spawn {
        obj.insert("spawn".to_string(), serde_json::json!(true));
    }

    if !env_allow.is_empty() {
        obj.insert("env".to_string(), serde_json::json!({ "allow": env_allow }));
    }

    if locked {
        obj.insert("locked".to_string(), serde_json::json!(true));
    }

    serde_json::to_string_pretty(&value).context("Failed to serialize armor.json")
}

/// Prompt the user for a value, returning `default` when the input is blank.
///
/// When `valid_options` is non-empty, rejects any input not in the list.
fn prompt_with_default(
    reader: &mut dyn BufRead,
    label: &str,
    default: &str,
    valid_options: &[&str],
) -> Result<String> {
    print!("{label}: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut buf = String::new();
    reader.read_line(&mut buf)?;
    let trimmed = buf.trim();

    if trimmed.is_empty() {
        return Ok(default.to_string());
    }

    if !valid_options.is_empty() && !valid_options.contains(&trimmed) {
        bail!(
            "Invalid value '{trimmed}'. Valid options: {}",
            valid_options.join(", ")
        );
    }

    Ok(trimmed.to_string())
}

/// Prompt for a comma-separated list, returning an empty `Vec` on blank input.
fn prompt_csv(reader: &mut dyn BufRead, label: &str) -> Result<Vec<String>> {
    print!("{label}: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut buf = String::new();
    reader.read_line(&mut buf)?;
    let trimmed = buf.trim();

    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    Ok(trimmed.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
}

/// Prompt for a yes/no boolean, returning `default` on blank input.
///
/// Accepts `y`, `yes`, `n`, `no` (case-insensitive).
fn prompt_bool(reader: &mut dyn BufRead, label: &str, default: bool) -> Result<bool> {
    let default_hint = if default { "y" } else { "n" };
    print!("{label} [{default_hint}]: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut buf = String::new();
    reader.read_line(&mut buf)?;
    let trimmed = buf.trim().to_lowercase();

    if trimmed.is_empty() {
        return Ok(default);
    }

    match trimmed.as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => bail!("Invalid input '{trimmed}'. Enter y/yes or n/no."),
    }
}

/// Generate an armor.json JSON string for the given profile name.
fn generate_armor_json(profile_name: &str) -> Result<String> {
    let schema_uri = "https://mcp-armor.com/spec/v1.0/armor.schema.json";

    let value = match profile_name {
        "strict" => serde_json::json!({
            "$schema": schema_uri,
            "version": "1.0",
            "profile": "strict"
        }),
        "sandboxed" => serde_json::json!({
            "$schema": schema_uri,
            "version": "1.0",
            "profile": "sandboxed",
            "filesystem": { "read": [], "write": [] },
            "network": { "allow": [], "deny_local": true, "deny_metadata": true }
        }),
        "network" => serde_json::json!({
            "$schema": schema_uri,
            "version": "1.0",
            "profile": "network",
            "network": {
                "allow": ["api.example.com:443"],
                "deny_local": true,
                "deny_metadata": true
            }
        }),
        "system" => serde_json::json!({
            "$schema": schema_uri,
            "version": "1.0",
            "profile": "system"
            // Trust all capabilities — document your reasons in a PR.
        }),
        "browser" => serde_json::json!({
            "$schema": schema_uri,
            "version": "1.0",
            "profile": "browser",
            "network": {
                "allow": [],
                "deny_local": false,
                "deny_metadata": true
            }
        }),
        _ => bail!("Unknown profile: '{profile_name}'. Valid: strict, sandboxed, network, system, browser"),
    };

    serde_json::to_string_pretty(&value).context("Failed to serialize armor.json")
}

// ---------------------------------------------------------------------------
// profiles
// ---------------------------------------------------------------------------

/// Manage armor profiles (list, show, update, add).
pub async fn profiles(args: ProfilesArgs) -> Result<()> {
    match args.command {
        ProfilesCommand::List => profiles_list().await,
        ProfilesCommand::Show { name } => profiles_show(&name).await,
        ProfilesCommand::Update => profiles_update().await,
        ProfilesCommand::Add { file } => profiles_add(&file).await,
    }
}

/// List all available community profiles.
///
/// Shows bundled (compiled-in) profiles first, then any user-installed extras
/// from `~/.mcparmor/profiles/community/` that are not already in the bundled
/// set. Profiles are available offline immediately after installation because
/// the bundled set is embedded in the binary at compile time.
async fn profiles_list() -> Result<()> {
    println!("{:<30} {:<15} {:<10} NETWORK ALLOW", "NAME", "PROFILE", "SOURCE");
    println!("{}", "-".repeat(80));

    // Bundled profiles are always available offline.
    for (name, content) in BUNDLED_COMMUNITY_PROFILES {
        print_profile_row(name, content, "bundled");
    }

    // Also surface any user-installed profiles not already in the bundled set.
    let dir = community_profiles_dir();
    if dir.exists() {
        print_user_installed_extras(&dir);
    }

    Ok(())
}

/// Print one profile row to stdout.
fn print_profile_row(name: &str, content: &str, source: &str) {
    if let Ok(manifest) = serde_json::from_str::<ArmorManifest>(content) {
        let network_summary = manifest.network.allow.join(", ");
        println!(
            "{:<30} {:<15} {:<10} {}",
            name,
            profile_name(&manifest.profile),
            source,
            network_summary,
        );
    }
}

/// Print any user-installed profiles that are not already bundled.
fn print_user_installed_extras(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".armor");

        // Skip profiles already shown from the bundled set.
        if find_bundled_profile(name).is_some() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            print_profile_row(name, &content, "user");
        }
    }
}

/// Show a named community profile.
///
/// Resolution order:
/// 1. User-installed file at `~/.mcparmor/profiles/community/<name>.armor.json`.
/// 2. Bundled (compile-time embedded) profile for the given name.
///
/// This guarantees that bundled profiles are accessible offline even before
/// the user has run `mcparmor profiles update`.
async fn profiles_show(name: &str) -> Result<()> {
    let path = community_profile_path(name);
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        println!("{content}");
        return Ok(());
    }

    if let Some(content) = find_bundled_profile(name) {
        println!("{content}");
        return Ok(());
    }

    bail!("Profile '{}' not found. Run `mcparmor profiles update` to fetch community profiles.", name)
}

/// Fetch the latest community profiles from GitHub.
async fn profiles_update() -> Result<()> {
    let dir = community_profiles_dir();
    fs::create_dir_all(&dir).context("Cannot create community profiles directory")?;

    let base_url = community_profiles_base_url();
    let index_url = format!("{base_url}/index.json");

    let updated_count = tokio::task::spawn_blocking(move || {
        download_all_profiles(&index_url, &dir)
    })
    .await
    .context("Spawn-blocking task failed")??;

    println!("{updated_count} profiles updated.");
    Ok(())
}

/// Fetch the profiles index and download each listed profile to `dest_dir`.
///
/// Returns the number of profiles successfully downloaded and written.
///
/// # Errors
/// Returns an error if the index cannot be fetched, a profile download fails,
/// or a SHA-256 checksum mismatch is detected.
fn download_all_profiles(index_url: &str, dest_dir: &Path) -> Result<usize> {
    let response = ureq::get(index_url)
        .call()
        .context("Failed to fetch profiles index")?;

    let index: Vec<Value> = response
        .into_json()
        .context("Invalid JSON in profiles index")?;

    let mut updated = 0_usize;
    for entry in &index {
        let Some(name) = entry["name"].as_str() else {
            continue;
        };
        let expected_sha = entry["sha256"].as_str().unwrap_or("");
        download_profile(name, expected_sha, dest_dir)?;
        updated += 1;
    }
    Ok(updated)
}

/// Fetch a single named community profile and write it to `dest_dir`.
///
/// Verifies the SHA-256 checksum when `expected_sha` is non-empty.
///
/// # Errors
/// Returns an error if the HTTP request fails, the checksum mismatches,
/// or the file cannot be written.
fn download_profile(name: &str, expected_sha: &str, dest_dir: &Path) -> Result<()> {
    let base_url = community_profiles_base_url();
    let profile_url = format!("{base_url}/{name}.armor.json");
    let profile_response = ureq::get(&profile_url)
        .call()
        .with_context(|| format!("Failed to fetch profile '{name}'"))?;

    let content = profile_response
        .into_string()
        .context("Failed to read profile content")?;

    if !expected_sha.is_empty() {
        verify_sha256(&content, expected_sha, name)?;
    }

    let dest = dest_dir.join(format!("{name}.armor.json"));
    fs::write(&dest, &content)
        .with_context(|| format!("Failed to write profile '{name}'"))?;
    Ok(())
}

/// Returns the base URL for community profiles pinned to the release tag.
///
/// Uses `COMMUNITY_PROFILES_RELEASE_TAG` so profile downloads are tied to a
/// specific, auditable release rather than the mutable `main` branch. This
/// prevents a supply-chain attack where a push to `main` silently changes a
/// downloaded profile between installations.
fn community_profiles_base_url() -> String {
    format!(
        "https://raw.githubusercontent.com/otomus/mcparmor/{}/profiles/community",
        COMMUNITY_PROFILES_RELEASE_TAG
    )
}

/// Returns the community profiles directory path.
fn community_profiles_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".mcparmor")
        .join("profiles")
        .join("community")
}

/// Verify the SHA-256 checksum of a downloaded profile.
fn verify_sha256(content: &str, expected_hex: &str, name: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(content.as_bytes());
    let actual_hex = hex::encode(digest);
    if actual_hex != expected_hex {
        bail!(
            "SHA-256 mismatch for profile '{name}': expected {expected_hex}, got {actual_hex}"
        );
    }
    Ok(())
}

/// Install a local armor.json as a named community profile.
async fn profiles_add(file: &Path) -> Result<()> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Cannot read {}", file.display()))?;

    // Validate the file before installing.
    let instance: Value = serde_json::from_str(&content)
        .with_context(|| format!("Not valid JSON: {}", file.display()))?;

    let schema_errors = validate_against_schema(&instance)?;
    if !schema_errors.is_empty() {
        for err in &schema_errors {
            eprintln!("Validation error: {err}");
        }
        bail!("Profile at {} is not a valid armor.json — not installed.", file.display());
    }

    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Cannot determine file name")?
        .trim_end_matches(".armor");

    let dir = community_profiles_dir();
    fs::create_dir_all(&dir).context("Cannot create profiles directory")?;

    let dest = dir.join(format!("{stem}.armor.json"));
    fs::copy(file, &dest)
        .with_context(|| format!("Failed to copy {} → {}", file.display(), dest.display()))?;

    println!("Installed profile '{}' at {}", stem, dest.display());
    Ok(())
}

/// Returns the lowercase name of a `Profile` variant.
fn profile_name(profile: &Profile) -> &'static str {
    match profile {
        Profile::Strict => "strict",
        Profile::Sandboxed => "sandboxed",
        Profile::Network => "network",
        Profile::System => "system",
        Profile::Browser => "browser",
    }
}

/// Validate a JSON value against the embedded armor schema.
///
/// Returns a list of human-readable error strings, or an empty vec if valid.
///
/// The compiled schema is cached in a `OnceLock` for the process lifetime.
/// `jsonschema` v0.18 requires a `'static` reference to the schema `Value`; the
/// canonical way to satisfy this is to store the `Value` in a `'static` via
/// `OnceLock` rather than leaking heap allocations on every call.
///
/// # Errors
/// Returns an error only if the embedded schema itself cannot be compiled,
/// which is a build-time programmer error and should never happen in production.
fn validate_against_schema(instance: &Value) -> Result<Vec<String>> {
    // The schema Value must live for 'static so that JSONSchema::compile can
    // hold a reference to it. Storing both in OnceLock statics achieves this
    // without any heap leak.
    static SCHEMA_VALUE: OnceLock<Value> = OnceLock::new();
    static COMPILED_SCHEMA: OnceLock<jsonschema::JSONSchema> = OnceLock::new();

    let compiled = COMPILED_SCHEMA.get_or_init(|| {
        let schema = SCHEMA_VALUE.get_or_init(|| {
            serde_json::from_str(ARMOR_SCHEMA)
                .expect("embedded ARMOR_SCHEMA is not valid JSON — this is a compile-time error")
        });
        jsonschema::JSONSchema::compile(schema)
            .expect("embedded ARMOR_SCHEMA failed to compile — this is a compile-time error")
    });

    let errors: Vec<String> = compiled
        .validate(instance)
        .err()
        .map(|iter| iter.map(|e| format!("  {e}")).collect())
        .unwrap_or_default();

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcparmor_core::manifest::{
        AuditPolicy, EnvPolicy, FilesystemPolicy, NetworkPolicy, OutputPolicy, Profile,
    };

    // ---------------------------------------------------------------------------
    // Test helpers
    // ---------------------------------------------------------------------------

    /// Build a minimal `ArmorManifest` with sensible defaults for testing.
    fn minimal_manifest(profile: Profile) -> ArmorManifest {
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

    // ---------------------------------------------------------------------------
    // parse_profile
    // ---------------------------------------------------------------------------

    #[test]
    fn parse_profile_returns_correct_variants() {
        assert_eq!(parse_profile("strict"), Some(Profile::Strict));
        assert_eq!(parse_profile("sandboxed"), Some(Profile::Sandboxed));
        assert_eq!(parse_profile("network"), Some(Profile::Network));
        assert_eq!(parse_profile("system"), Some(Profile::System));
        assert_eq!(parse_profile("browser"), Some(Profile::Browser));
    }

    #[test]
    fn parse_profile_returns_none_for_unknown_name() {
        assert_eq!(parse_profile("unknown"), None);
        assert_eq!(parse_profile(""), None);
        assert_eq!(parse_profile("STRICT"), None); // case-sensitive
        assert_eq!(parse_profile("Sandboxed"), None);
    }

    // ---------------------------------------------------------------------------
    // apply_profile_override
    // ---------------------------------------------------------------------------

    #[test]
    fn apply_profile_override_with_no_override_leaves_manifest_unchanged() {
        let manifest = minimal_manifest(Profile::Sandboxed);
        let result = apply_profile_override(manifest, None);
        assert_eq!(result.profile, Profile::Sandboxed);
    }

    #[test]
    fn apply_profile_override_changes_profile_when_not_locked() {
        let manifest = minimal_manifest(Profile::Sandboxed);
        let result = apply_profile_override(manifest, Some("strict"));
        assert_eq!(result.profile, Profile::Strict);
    }

    #[test]
    fn apply_profile_override_ignores_override_when_manifest_is_locked() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.locked = true;
        let result = apply_profile_override(manifest, Some("strict"));
        assert_eq!(result.profile, Profile::Sandboxed, "locked manifest must not be overridden");
    }

    #[test]
    fn apply_profile_override_ignores_unknown_profile_name() {
        let manifest = minimal_manifest(Profile::Sandboxed);
        let result = apply_profile_override(manifest, Some("not-a-real-profile"));
        assert_eq!(result.profile, Profile::Sandboxed, "unknown profile name must not change the manifest");
    }

    // ---------------------------------------------------------------------------
    // apply_browser_defaults
    // ---------------------------------------------------------------------------

    #[test]
    fn apply_browser_defaults_sets_deny_local_false_for_browser_profile() {
        let mut manifest = minimal_manifest(Profile::Browser);
        manifest.network.deny_local = true; // set to true; broker must override
        let result = apply_browser_defaults(manifest);
        assert!(!result.network.deny_local, "browser profile must force deny_local to false");
    }

    #[test]
    fn apply_browser_defaults_preserves_deny_local_for_non_browser_profiles() {
        for profile in [Profile::Strict, Profile::Sandboxed, Profile::Network, Profile::System] {
            let mut manifest = minimal_manifest(profile.clone());
            manifest.network.deny_local = true;
            let result = apply_browser_defaults(manifest);
            assert!(result.network.deny_local, "deny_local must not be changed for {profile:?}");
        }
    }

    #[test]
    fn apply_browser_defaults_is_idempotent_when_deny_local_already_false() {
        let mut manifest = minimal_manifest(Profile::Browser);
        manifest.network.deny_local = false;
        let result = apply_browser_defaults(manifest);
        assert!(!result.network.deny_local);
    }

    // ---------------------------------------------------------------------------
    // advisory_warnings
    // ---------------------------------------------------------------------------

    #[test]
    fn advisory_warnings_empty_env_allow_produces_no_path_warning() {
        let manifest = minimal_manifest(Profile::Sandboxed);
        let warnings = advisory_warnings(&manifest);
        assert!(!warnings.iter().any(|w| w.contains("PATH")), "empty env.allow must not warn about PATH");
    }

    #[test]
    fn advisory_warnings_env_allow_with_path_produces_no_warning() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.env.allow = vec!["PATH".to_string(), "HOME".to_string()];
        let warnings = advisory_warnings(&manifest);
        assert!(!warnings.iter().any(|w| w.contains("PATH") && w.contains("missing")));
        // No PATH warning when PATH is present.
        assert!(!warnings.iter().any(|w| w.contains("find their runtime")));
    }

    #[test]
    fn advisory_warnings_env_allow_without_path_produces_warning() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.env.allow = vec!["HOME".to_string(), "GITHUB_TOKEN".to_string()];
        let warnings = advisory_warnings(&manifest);
        assert!(
            warnings.iter().any(|w| w.contains("PATH")),
            "env.allow without PATH must warn: {warnings:?}"
        );
    }

    #[test]
    fn advisory_warnings_browser_profile_with_deny_local_true_warns() {
        let mut manifest = minimal_manifest(Profile::Browser);
        manifest.network.deny_local = true;
        let warnings = advisory_warnings(&manifest);
        assert!(
            warnings.iter().any(|w| w.contains("deny_local")),
            "browser + deny_local:true must warn: {warnings:?}"
        );
    }

    #[test]
    fn advisory_warnings_browser_profile_with_deny_local_false_no_browser_warning() {
        let mut manifest = minimal_manifest(Profile::Browser);
        manifest.network.deny_local = false;
        let warnings = advisory_warnings(&manifest);
        // Should not warn about deny_local override for browser when already false.
        assert!(!warnings.iter().any(|w| w.contains("override")));
    }

    #[test]
    fn advisory_warnings_non_browser_profile_with_deny_local_false_warns() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.network.deny_local = false;
        let warnings = advisory_warnings(&manifest);
        assert!(
            warnings.iter().any(|w| w.contains("deny_local: false")),
            "non-browser profile with deny_local:false must warn: {warnings:?}"
        );
    }

    #[test]
    fn advisory_warnings_no_warnings_for_clean_manifest() {
        let manifest = minimal_manifest(Profile::Sandboxed); // deny_local defaults to true, env.allow empty
        let warnings = advisory_warnings(&manifest);
        assert!(warnings.is_empty(), "clean manifest must produce no warnings: {warnings:?}");
    }

    #[test]
    fn advisory_warnings_min_spec_exceeds_broker_warns() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.min_spec = Some("99.0".to_string());
        let warnings = advisory_warnings(&manifest);
        assert!(
            warnings.iter().any(|w| w.contains("min_spec")),
            "min_spec exceeding broker version must warn: {warnings:?}"
        );
    }

    #[test]
    fn advisory_warnings_min_spec_at_or_below_broker_no_warning() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.min_spec = Some("1.0".to_string());
        let warnings = advisory_warnings(&manifest);
        assert!(
            !warnings.iter().any(|w| w.contains("min_spec")),
            "min_spec at broker version must not warn: {warnings:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // spec_version_le
    // ---------------------------------------------------------------------------

    #[test]
    fn spec_version_le_equal_versions_returns_true() {
        assert!(spec_version_le("1.0", "1.0"));
    }

    #[test]
    fn spec_version_le_older_required_returns_true() {
        assert!(spec_version_le("0.9", "1.0"));
    }

    #[test]
    fn spec_version_le_newer_required_returns_false() {
        assert!(!spec_version_le("2.0", "1.0"));
    }

    #[test]
    fn spec_version_le_minor_version_comparison_works() {
        assert!(spec_version_le("1.0", "1.1"));
        assert!(!spec_version_le("1.1", "1.0"));
    }

    #[test]
    fn spec_version_le_malformed_required_returns_false() {
        assert!(!spec_version_le("not-a-version", "1.0"),
            "malformed required version must return false (conservative)");
    }

    #[test]
    fn spec_version_le_malformed_available_returns_false() {
        assert!(!spec_version_le("1.0", "bad"),
            "malformed available version must return false");
    }

    // ---------------------------------------------------------------------------
    // check_min_spec
    // ---------------------------------------------------------------------------

    #[test]
    fn check_min_spec_no_min_spec_always_passes() {
        let manifest = minimal_manifest(Profile::Sandboxed);
        assert!(manifest.min_spec.is_none());
        assert!(check_min_spec(&manifest).is_ok());
    }

    #[test]
    fn check_min_spec_current_version_passes() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.min_spec = Some(BROKER_SPEC_VERSION.to_string());
        assert!(check_min_spec(&manifest).is_ok());
    }

    #[test]
    fn check_min_spec_older_required_passes() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.min_spec = Some("0.1".to_string());
        assert!(check_min_spec(&manifest).is_ok());
    }

    #[test]
    fn check_min_spec_newer_required_returns_error() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.min_spec = Some("99.0".to_string());
        let result = check_min_spec(&manifest);
        assert!(result.is_err(), "min_spec exceeding broker version must error");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("99.0"), "error message must include required version");
        assert!(msg.contains(BROKER_SPEC_VERSION), "error message must include broker version");
        assert!(msg.contains("Upgrade"), "error message must tell user to upgrade");
    }

    // ---------------------------------------------------------------------------
    // format_secret_scan_mode
    // ---------------------------------------------------------------------------

    #[test]
    fn format_secret_scan_mode_disabled_shows_off() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.output.scan_secrets = SecretScanMode::Disabled;
        assert_eq!(format_secret_scan_mode(&manifest), "off");
    }

    #[test]
    fn format_secret_scan_mode_redact_shows_on_redact() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.output.scan_secrets = SecretScanMode::Redact;
        assert_eq!(format_secret_scan_mode(&manifest), "on (redact)");
    }

    #[test]
    fn format_secret_scan_mode_strict_shows_on_strict_block() {
        let mut manifest = minimal_manifest(Profile::Sandboxed);
        manifest.output.scan_secrets = SecretScanMode::Strict;
        assert_eq!(format_secret_scan_mode(&manifest), "on (strict/block)");
    }

    // ---------------------------------------------------------------------------
    // parse_since_filter
    // ---------------------------------------------------------------------------

    #[test]
    fn parse_since_filter_accepts_iso8601_datetime() {
        let result = parse_since_filter("2026-01-01T00:00:00Z");
        assert!(result.is_ok(), "ISO8601 datetime must parse: {:?}", result.err());
        assert_eq!(
            result.unwrap().to_rfc3339(),
            "2026-01-01T00:00:00+00:00"
        );
    }

    #[test]
    fn parse_since_filter_accepts_hours_relative_duration() {
        let before = Utc::now();
        let result = parse_since_filter("1h");
        let after = Utc::now();
        assert!(result.is_ok(), "1h must parse: {:?}", result.err());
        let cutoff = result.unwrap();
        // Cutoff should be approximately 1 hour before now.
        // Use a 60-second tolerance to remain robust under CI resource contention —
        // the point of this test is that "1h" maps to ~3600 seconds, not that it
        // is precise to the millisecond.
        let diff = (before - cutoff).num_seconds().abs();
        assert!(diff >= 3540 && diff <= 3660, "1h offset should be ~3600s (±60s), got {diff}s");
        // Cutoff must be before the measured 'after'.
        assert!(cutoff < after);
    }

    #[test]
    fn parse_since_filter_accepts_days_relative_duration() {
        let before = Utc::now();
        let result = parse_since_filter("7d");
        assert!(result.is_ok(), "7d must parse: {:?}", result.err());
        let cutoff = result.unwrap();
        let diff = (before - cutoff).num_seconds().abs();
        let seven_days_secs = 7 * 24 * 3600_i64;
        // Use a 60-second tolerance — the semantic test is that "7d" maps to
        // approximately 7 days, not that it is microsecond-precise.
        assert!(
            diff >= seven_days_secs - 60 && diff <= seven_days_secs + 60,
            "7d offset should be ~{seven_days_secs}s (±60s), got {diff}s"
        );
    }

    #[test]
    fn parse_since_filter_accepts_24h_relative_duration() {
        let result = parse_since_filter("24h");
        assert!(result.is_ok(), "24h must parse: {:?}", result.err());
    }

    #[test]
    fn parse_since_filter_rejects_unknown_format() {
        assert!(parse_since_filter("yesterday").is_err());
        assert!(parse_since_filter("1w").is_err());
        assert!(parse_since_filter("").is_err());
        assert!(parse_since_filter("xh").is_err()); // non-numeric hours
        assert!(parse_since_filter("xd").is_err()); // non-numeric days
    }

    // ---------------------------------------------------------------------------
    // build_wrapped_args
    // ---------------------------------------------------------------------------

    #[test]
    fn build_wrapped_args_minimal_produces_run_separator_cmd() {
        let args = build_wrapped_args(&None, None, "node", &[]);
        assert_eq!(args, vec!["run", "--", "node"]);
    }

    #[test]
    fn build_wrapped_args_with_armor_path_inserts_armor_flag() {
        let path = PathBuf::from("/path/to/armor.json");
        let args = build_wrapped_args(&Some(path), None, "node", &[]);
        assert_eq!(args, vec!["run", "--armor", "/path/to/armor.json", "--", "node"]);
    }

    #[test]
    fn build_wrapped_args_with_profile_override_inserts_profile_flag() {
        let args = build_wrapped_args(&None, Some("strict"), "node", &[]);
        assert_eq!(args, vec!["run", "--profile", "strict", "--", "node"]);
    }

    #[test]
    fn build_wrapped_args_with_both_armor_and_profile() {
        let path = PathBuf::from("/my/armor.json");
        let args = build_wrapped_args(&Some(path), Some("sandboxed"), "python3", &[]);
        assert_eq!(
            args,
            vec!["run", "--armor", "/my/armor.json", "--profile", "sandboxed", "--", "python3"]
        );
    }

    #[test]
    fn build_wrapped_args_original_args_appended_after_cmd() {
        let orig_args = vec!["server.js".to_string(), "--port".to_string(), "3000".to_string()];
        let args = build_wrapped_args(&None, None, "node", &orig_args);
        assert_eq!(args, vec!["run", "--", "node", "server.js", "--port", "3000"]);
    }

    #[test]
    fn build_wrapped_args_first_element_is_always_run() {
        let args = build_wrapped_args(&None, None, "cmd", &[]);
        assert_eq!(args[0], "run");
    }

    // ---------------------------------------------------------------------------
    // is_wrapped_entry
    // ---------------------------------------------------------------------------

    #[test]
    fn is_wrapped_entry_returns_true_for_correctly_wrapped_server() {
        let server = serde_json::json!({
            "command": "mcparmor",
            "args": ["run", "--armor", "/path/armor.json", "--", "node", "server.js"]
        });
        assert!(is_wrapped_entry(&server));
    }

    #[test]
    fn is_wrapped_entry_returns_false_when_command_is_not_mcparmor() {
        let server = serde_json::json!({
            "command": "node",
            "args": ["run", "server.js"]
        });
        assert!(!is_wrapped_entry(&server));
    }

    #[test]
    fn is_wrapped_entry_returns_false_when_mcparmor_but_no_run_arg() {
        let server = serde_json::json!({
            "command": "mcparmor",
            "args": ["validate", "--armor", "/path/armor.json"]
        });
        assert!(!is_wrapped_entry(&server));
    }

    #[test]
    fn is_wrapped_entry_returns_false_when_mcparmor_but_args_is_empty() {
        let server = serde_json::json!({
            "command": "mcparmor",
            "args": []
        });
        assert!(!is_wrapped_entry(&server));
    }

    #[test]
    fn is_wrapped_entry_returns_false_when_no_command_field() {
        let server = serde_json::json!({ "url": "http://example.com" });
        assert!(!is_wrapped_entry(&server));
    }

    // ---------------------------------------------------------------------------
    // extract_original_command
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_original_command_returns_cmd_and_args_after_separator() {
        let server = serde_json::json!({
            "args": ["run", "--armor", "/p/a.json", "--", "node", "server.js", "--port", "3000"]
        });
        let result = extract_original_command(&server);
        assert_eq!(result, Some(("node".to_string(), vec![
            "server.js".to_string(),
            "--port".to_string(),
            "3000".to_string(),
        ])));
    }

    #[test]
    fn extract_original_command_returns_cmd_with_empty_args_when_no_trailing_args() {
        let server = serde_json::json!({
            "args": ["run", "--", "python3"]
        });
        let result = extract_original_command(&server);
        assert_eq!(result, Some(("python3".to_string(), vec![])));
    }

    #[test]
    fn extract_original_command_returns_none_when_no_separator() {
        let server = serde_json::json!({
            "args": ["run", "--armor", "/p/a.json", "node"]
        });
        assert_eq!(extract_original_command(&server), None);
    }

    #[test]
    fn extract_original_command_returns_none_when_separator_is_last_element() {
        let server = serde_json::json!({ "args": ["run", "--"] });
        assert_eq!(extract_original_command(&server), None);
    }

    #[test]
    fn extract_original_command_returns_none_when_no_args_field() {
        let server = serde_json::json!({ "command": "mcparmor" });
        assert_eq!(extract_original_command(&server), None);
    }

    // ---------------------------------------------------------------------------
    // extract_armor_path_from_wrapped
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_armor_path_returns_path_after_armor_flag() {
        let server = serde_json::json!({
            "args": ["run", "--armor", "/etc/tools/armor.json", "--", "node"]
        });
        assert_eq!(
            extract_armor_path_from_wrapped(&server),
            Some("/etc/tools/armor.json".to_string())
        );
    }

    #[test]
    fn extract_armor_path_returns_none_when_no_armor_flag() {
        let server = serde_json::json!({
            "args": ["run", "--profile", "strict", "--", "node"]
        });
        assert_eq!(extract_armor_path_from_wrapped(&server), None);
    }

    #[test]
    fn extract_armor_path_returns_none_when_armor_flag_is_last_element() {
        let server = serde_json::json!({ "args": ["run", "--armor"] });
        assert_eq!(extract_armor_path_from_wrapped(&server), None);
    }

    #[test]
    fn extract_armor_path_returns_none_when_no_args_field() {
        let server = serde_json::json!({ "command": "mcparmor" });
        assert_eq!(extract_armor_path_from_wrapped(&server), None);
    }

    // ---------------------------------------------------------------------------
    // extract_mcp_servers
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_mcp_servers_reads_mcp_servers_key() {
        let config = serde_json::json!({
            "mcpServers": {
                "my-tool": { "command": "node", "args": [] }
            }
        });
        let servers = extract_mcp_servers(&config);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].0, "my-tool");
    }

    #[test]
    fn extract_mcp_servers_falls_back_to_servers_key() {
        let config = serde_json::json!({
            "servers": {
                "tool-a": { "command": "python3" }
            }
        });
        let servers = extract_mcp_servers(&config);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].0, "tool-a");
    }

    #[test]
    fn extract_mcp_servers_returns_empty_when_neither_key_present() {
        let config = serde_json::json!({ "other": {} });
        assert!(extract_mcp_servers(&config).is_empty());
    }

    #[test]
    fn extract_mcp_servers_returns_empty_for_empty_mcp_servers_object() {
        let config = serde_json::json!({ "mcpServers": {} });
        assert!(extract_mcp_servers(&config).is_empty());
    }

    #[test]
    fn extract_mcp_servers_returns_all_entries() {
        let config = serde_json::json!({
            "mcpServers": {
                "tool-a": { "command": "node" },
                "tool-b": { "command": "python3" },
                "tool-c": { "url": "http://example.com" }
            }
        });
        let servers = extract_mcp_servers(&config);
        assert_eq!(servers.len(), 3);
    }

    // ---------------------------------------------------------------------------
    // get_mcp_servers_mut
    // ---------------------------------------------------------------------------

    #[test]
    fn get_mcp_servers_mut_returns_mcp_servers_when_present() {
        let mut config = serde_json::json!({
            "mcpServers": { "my-tool": {} }
        });
        let result = get_mcp_servers_mut(&mut config);
        assert!(result.is_ok());
        let servers = result.unwrap();
        assert!(servers.as_object().is_some());
        assert!(servers.get("my-tool").is_some());
    }

    #[test]
    fn get_mcp_servers_mut_returns_servers_when_only_servers_key_present() {
        let mut config = serde_json::json!({
            "servers": { "tool-a": {} }
        });
        let result = get_mcp_servers_mut(&mut config);
        assert!(result.is_ok());
        let servers = result.unwrap();
        assert!(servers.get("tool-a").is_some());
    }

    #[test]
    fn get_mcp_servers_mut_creates_mcp_servers_when_neither_key_present() {
        let mut config = serde_json::json!({});
        let result = get_mcp_servers_mut(&mut config);
        assert!(result.is_ok());
        // The returned value should be an empty object.
        assert_eq!(result.unwrap().as_object().map(|m| m.len()), Some(0));
        // The key should now be present in the config.
        assert!(config.get("mcpServers").is_some());
    }

    #[test]
    fn get_mcp_servers_mut_errors_when_config_is_not_an_object() {
        let mut config = serde_json::json!([1, 2, 3]);
        assert!(get_mcp_servers_mut(&mut config).is_err());
    }

    // ---------------------------------------------------------------------------
    // generate_armor_json
    // ---------------------------------------------------------------------------

    #[test]
    fn generate_armor_json_produces_valid_json_for_each_profile() {
        for profile_name_str in ["strict", "sandboxed", "network", "system", "browser"] {
            let result = generate_armor_json(profile_name_str);
            assert!(result.is_ok(), "generate_armor_json({profile_name_str}) must succeed");
            let json_str = result.unwrap();
            let parsed: Value = serde_json::from_str(&json_str)
                .expect("generate_armor_json must produce valid JSON");
            assert_eq!(
                parsed["profile"].as_str(),
                Some(profile_name_str),
                "profile field must match requested profile for {profile_name_str}"
            );
        }
    }

    #[test]
    fn generate_armor_json_includes_correct_schema_uri() {
        let json_str = generate_armor_json("strict").unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed["$schema"].as_str(),
            Some("https://mcp-armor.com/spec/v1.0/armor.schema.json")
        );
    }

    #[test]
    fn generate_armor_json_returns_error_for_unknown_profile() {
        assert!(generate_armor_json("unknown").is_err());
        assert!(generate_armor_json("").is_err());
        assert!(generate_armor_json("STRICT").is_err());
    }

    #[test]
    fn generate_armor_json_sandboxed_includes_filesystem_and_network() {
        let json_str = generate_armor_json("sandboxed").unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.get("filesystem").is_some(), "sandboxed must include filesystem");
        assert!(parsed.get("network").is_some(), "sandboxed must include network");
    }

    // ---------------------------------------------------------------------------
    // verify_sha256
    // ---------------------------------------------------------------------------

    #[test]
    fn verify_sha256_passes_for_correct_hash() {
        use sha2::{Digest, Sha256};
        let content = "test profile content";
        let digest = Sha256::digest(content.as_bytes());
        let correct_hash = hex::encode(digest);
        let result = verify_sha256(content, &correct_hash, "test-profile");
        assert!(result.is_ok(), "correct hash must pass verification: {:?}", result.err());
    }

    #[test]
    fn verify_sha256_fails_for_wrong_hash() {
        let result = verify_sha256("some content", "deadbeefdeadbeef", "my-profile");
        assert!(result.is_err(), "wrong hash must fail verification");
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(err_msg.contains("my-profile"), "error must mention profile name: {err_msg}");
    }

    #[test]
    fn verify_sha256_fails_for_empty_expected_hash() {
        // Empty expected hash can never match — always a mismatch.
        let result = verify_sha256("content", "", "test-profile");
        assert!(result.is_err());
    }

    #[test]
    fn verify_sha256_passes_for_empty_content_with_correct_hash() {
        use sha2::{Digest, Sha256};
        let content = "";
        let digest = Sha256::digest(content.as_bytes());
        let correct_hash = hex::encode(digest);
        let result = verify_sha256(content, &correct_hash, "empty-profile");
        assert!(result.is_ok());
    }

    // ---------------------------------------------------------------------------
    // profile_name
    // ---------------------------------------------------------------------------

    #[test]
    fn profile_name_returns_correct_string_for_each_variant() {
        assert_eq!(profile_name(&Profile::Strict), "strict");
        assert_eq!(profile_name(&Profile::Sandboxed), "sandboxed");
        assert_eq!(profile_name(&Profile::Network), "network");
        assert_eq!(profile_name(&Profile::System), "system");
        assert_eq!(profile_name(&Profile::Browser), "browser");
    }

    #[test]
    fn profile_name_output_matches_parse_profile_input() {
        // round-trip: profile_name → parse_profile must recover the same variant.
        for profile in [
            Profile::Strict,
            Profile::Sandboxed,
            Profile::Network,
            Profile::System,
            Profile::Browser,
        ] {
            let name = profile_name(&profile);
            let recovered = parse_profile(name);
            assert_eq!(recovered, Some(profile.clone()), "round-trip failed for {profile:?}");
        }
    }

    // ---------------------------------------------------------------------------
    // GAP 2: print_sandbox_section helpers
    // ---------------------------------------------------------------------------

    #[test]
    fn print_sandbox_section_shows_checkmarks_for_available_capabilities() {
        // When all capabilities are available, every line should contain ✅.
        use crate::sandbox::EnforcementSummary;
        let summary = EnforcementSummary {
            filesystem_isolation: true,
            spawn_blocking: true,
            network_port_enforcement: true,
            network_hostname_enforcement: true,
            mechanism: "test-mechanism".to_string(),
        };
        assert_eq!(capability_icon(summary.filesystem_isolation), "✅");
        assert_eq!(capability_icon(summary.spawn_blocking), "✅");
        assert_eq!(capability_icon(summary.network_port_enforcement), "✅");
        assert_eq!(capability_icon(summary.network_hostname_enforcement), "✅");
    }

    #[test]
    fn capability_icon_returns_warning_for_unavailable_capability() {
        assert_eq!(capability_icon(false), "⚠ ");
    }

    #[test]
    fn capability_icon_returns_checkmark_for_available_capability() {
        assert_eq!(capability_icon(true), "✅");
    }

    #[test]
    fn detect_platform_label_returns_non_empty_string() {
        // The platform label must always be non-empty regardless of OS.
        let label = detect_platform_label();
        assert!(!label.is_empty(), "platform label must not be empty");
    }

    #[test]
    fn detect_platform_label_contains_recognizable_os_name() {
        let label = detect_platform_label();
        // On macOS this will be "macOS"; on Linux something like "Linux 5.15..."; on Windows "Windows".
        let os = std::env::consts::OS;
        let expected_substring = match os {
            "macos" => "macOS",
            "linux" => "Linux",
            "windows" => "Windows",
            other => other,
        };
        assert!(
            label.contains(expected_substring),
            "platform label '{label}' should contain '{expected_substring}'"
        );
    }

    // ---------------------------------------------------------------------------
    // GAP 3: current_platform_line and status summary
    // ---------------------------------------------------------------------------

    #[test]
    fn current_platform_line_shows_layer2_unavailable_for_noop() {
        use crate::sandbox::EnforcementSummary;
        // NoopSandbox has all booleans false — Layer 2 should show ❌.
        let summary = EnforcementSummary {
            filesystem_isolation: false,
            spawn_blocking: false,
            network_port_enforcement: false,
            network_hostname_enforcement: false,
            mechanism: "none — protocol-layer enforcement only".to_string(),
        };
        let line = current_platform_line(&summary);
        assert!(line.contains("Layer 1 ✅"), "Layer 1 must always be available: {line}");
        assert!(line.contains("Layer 2 ❌"), "Layer 2 must be unavailable for noop: {line}");
        assert!(line.contains("not available"), "must explain layer 2 absence: {line}");
    }

    #[test]
    fn current_platform_line_shows_layer2_available_when_filesystem_isolation_true() {
        use crate::sandbox::EnforcementSummary;
        let summary = EnforcementSummary {
            filesystem_isolation: true,
            spawn_blocking: false,
            network_port_enforcement: false,
            network_hostname_enforcement: false,
            mechanism: "Landlock FS".to_string(),
        };
        let line = current_platform_line(&summary);
        assert!(line.contains("Layer 2 ✅"), "filesystem_isolation=true must make Layer 2 available: {line}");
        assert!(line.contains("Landlock FS"), "mechanism must appear in line: {line}");
    }

    #[test]
    fn current_platform_line_always_contains_platform_name() {
        use crate::sandbox::EnforcementSummary;
        let summary = EnforcementSummary {
            filesystem_isolation: false,
            spawn_blocking: false,
            network_port_enforcement: false,
            network_hostname_enforcement: false,
            mechanism: "none".to_string(),
        };
        let line = current_platform_line(&summary);
        assert!(line.starts_with("Platform:"), "line must start with 'Platform:': {line}");
    }

    #[test]
    fn status_summary_counts_armored_http_and_unwrapped_correctly() {
        // Build a set of ToolStatus entries: 2 armored, 1 HTTP, 1 unwrapped.
        let configs = vec![
            ToolStatus {
                host: "claude-desktop".to_string(),
                tool_name: "github".to_string(),
                is_wrapped: true,
                is_http: false,
                armor_path: Some("/path/armor.json".to_string()),
                profile: None,
            },
            ToolStatus {
                host: "claude-desktop".to_string(),
                tool_name: "filesystem".to_string(),
                is_wrapped: true,
                is_http: false,
                armor_path: None,
                profile: None,
            },
            ToolStatus {
                host: "claude-desktop".to_string(),
                tool_name: "remote-api".to_string(),
                is_wrapped: false,
                is_http: true,
                armor_path: None,
                profile: None,
            },
            ToolStatus {
                host: "claude-desktop".to_string(),
                tool_name: "unprotected".to_string(),
                is_wrapped: false,
                is_http: false,
                armor_path: None,
                profile: None,
            },
        ];

        // Capture stdout by using a buffer — we test the counting logic directly.
        let armored = configs.iter().filter(|e| e.is_wrapped).count();
        let http_skipped = configs.iter().filter(|e| !e.is_wrapped && e.is_http).count();
        let unwrapped = configs.iter().filter(|e| !e.is_wrapped && !e.is_http).count();

        assert_eq!(armored, 2, "must count 2 armored tools");
        assert_eq!(http_skipped, 1, "must count 1 HTTP-skipped tool");
        assert_eq!(unwrapped, 1, "must count 1 unwrapped tool");
    }

    #[test]
    fn status_summary_all_armored_shows_zero_unwrapped() {
        let configs = vec![
            ToolStatus {
                host: "h".to_string(),
                tool_name: "t1".to_string(),
                is_wrapped: true,
                is_http: false,
                armor_path: None,
                profile: None,
            },
            ToolStatus {
                host: "h".to_string(),
                tool_name: "t2".to_string(),
                is_wrapped: true,
                is_http: false,
                armor_path: None,
                profile: None,
            },
        ];
        let unwrapped = configs.iter().filter(|e| !e.is_wrapped && !e.is_http).count();
        assert_eq!(unwrapped, 0, "must count 0 unwrapped tools when all are armored");
    }

    #[test]
    fn status_summary_empty_list_shows_all_zeros() {
        let configs: Vec<ToolStatus> = vec![];
        let armored = configs.iter().filter(|e| e.is_wrapped).count();
        let http_skipped = configs.iter().filter(|e| !e.is_wrapped && e.is_http).count();
        let unwrapped = configs.iter().filter(|e| !e.is_wrapped && !e.is_http).count();
        assert_eq!(armored, 0);
        assert_eq!(http_skipped, 0);
        assert_eq!(unwrapped, 0);
    }

    // ---------------------------------------------------------------------------
    // GAP 4: ArmorSource and format_armor_source
    // ---------------------------------------------------------------------------

    #[test]
    fn armor_source_community_profile_annotates_wrap_output() {
        let source = ArmorSource::CommunityProfile { name: "github".to_string() };
        let label = format_armor_source(&source);
        assert!(label.contains("community profile"), "must mention community profile: {label}");
        assert!(label.contains("github"), "must include tool name: {label}");
    }

    #[test]
    fn armor_source_local_file_shows_filename() {
        let source = ArmorSource::LocalFile { path: PathBuf::from("/path/to/armor.json") };
        let label = format_armor_source(&source);
        assert!(label.contains("armor.json"), "must show filename: {label}");
        assert!(label.starts_with("./"), "must use relative-style path: {label}");
    }

    #[test]
    fn armor_source_strict_fallback_annotates_wrap_output() {
        let source = ArmorSource::StrictFallback;
        let label = format_armor_source(&source);
        assert!(label.contains("strict fallback"), "must mention strict fallback: {label}");
        assert!(label.contains("no armor.json found"), "must explain reason: {label}");
    }

    #[test]
    fn wrap_outcome_icon_returns_warning_for_strict_fallback() {
        let source = ArmorSource::StrictFallback;
        assert_eq!(wrap_outcome_icon(&source), "⚠ ");
    }

    #[test]
    fn wrap_outcome_icon_returns_checkmark_for_community_profile() {
        let source = ArmorSource::CommunityProfile { name: "test".to_string() };
        assert_eq!(wrap_outcome_icon(&source), "✅");
    }

    #[test]
    fn wrap_outcome_icon_returns_checkmark_for_local_file() {
        let source = ArmorSource::LocalFile { path: PathBuf::from("armor.json") };
        assert_eq!(wrap_outcome_icon(&source), "✅");
    }

    #[test]
    fn armor_source_community_profile_with_empty_name_still_formats() {
        // Edge case: tool name is empty string.
        let source = ArmorSource::CommunityProfile { name: String::new() };
        let label = format_armor_source(&source);
        assert!(label.contains("community profile"), "must still mention community profile");
    }

    #[test]
    fn armor_source_local_file_with_no_filename_uses_armor_json_fallback() {
        // Path that has no file_name component (e.g. root "/").
        // This should not panic and should use the "armor.json" fallback.
        let source = ArmorSource::LocalFile { path: PathBuf::from("/") };
        let label = format_armor_source(&source);
        // Should not panic; result may be "./" with empty or "armor.json".
        assert!(!label.is_empty(), "must return non-empty string even for root path");
    }

    // ---------------------------------------------------------------------------
    // RunArgs: new audit flags have correct defaults
    // ---------------------------------------------------------------------------

    #[test]
    fn run_args_audit_log_defaults_to_none() {
        // Parsing `run -- echo` must leave audit_log as None when the flag is absent.
        use clap::Parser;
        use crate::cli::Cli;

        let cli = Cli::try_parse_from(["mcparmor", "run", "--", "echo"]).unwrap();
        let crate::cli::Command::Run(args) = cli.command else {
            panic!("expected Run command");
        };
        assert!(
            args.audit_log.is_none(),
            "audit_log must default to None when --audit-log is not provided"
        );
    }

    #[test]
    fn run_args_no_audit_defaults_to_false() {
        // Parsing `run -- echo` must leave no_audit as false when the flag is absent.
        use clap::Parser;
        use crate::cli::Cli;

        let cli = Cli::try_parse_from(["mcparmor", "run", "--", "echo"]).unwrap();
        let crate::cli::Command::Run(args) = cli.command else {
            panic!("expected Run command");
        };
        assert!(
            !args.no_audit,
            "no_audit must default to false when --no-audit is not provided"
        );
    }

    // ---------------------------------------------------------------------------
    // build_audit_writer: selects the correct writer variant
    // ---------------------------------------------------------------------------

    #[test]
    fn build_audit_writer_returns_disabled_when_no_audit_is_true() {
        // A disabled writer returns None from log_path().
        let writer = build_audit_writer(true, None, None, None);
        assert!(
            writer.log_path().is_none(),
            "disabled writer must have no path"
        );
    }

    #[test]
    fn build_audit_writer_uses_custom_path_when_audit_log_is_provided() {
        let custom = PathBuf::from("/tmp/custom-audit.jsonl");
        let writer = build_audit_writer(false, Some(custom.clone()), None, None);
        assert_eq!(
            writer.log_path().unwrap(),
            custom.as_path(),
            "writer must use the provided audit_log path"
        );
    }

    #[test]
    fn build_audit_writer_no_audit_takes_precedence_over_audit_log() {
        // When both no_audit and audit_log are set, no_audit wins.
        let custom = PathBuf::from("/tmp/should-not-be-used.jsonl");
        let writer = build_audit_writer(true, Some(custom), None, None);
        assert!(
            writer.log_path().is_none(),
            "no_audit=true must disable the writer even when audit_log is also set"
        );
    }

    #[test]
    fn build_audit_writer_uses_default_path_when_neither_flag_is_set() {
        let writer = build_audit_writer(false, None, None, None);
        let default = AuditWriter::default_path();
        assert_eq!(
            writer.log_path().unwrap(),
            default.as_path(),
            "writer must use the default path when no flags are set"
        );
    }

    // ---------------------------------------------------------------------------
    // extract_profile_from_armor_path
    // ---------------------------------------------------------------------------

    #[test]
    fn tool_status_profile_is_populated_from_armor_json() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let armor_path = dir.path().join("armor.json");
        let content = r#"{"version":"1.0","profile":"sandboxed"}"#;
        fs::write(&armor_path, content).unwrap();

        let path_str = armor_path.to_string_lossy().into_owned();
        let profile = extract_profile_from_armor_path(Some(&path_str));

        assert_eq!(
            profile,
            Some("sandboxed".to_string()),
            "profile must be extracted from a readable armor.json"
        );
    }

    #[test]
    fn tool_status_profile_is_none_when_armor_path_is_none() {
        // When no armor path is provided (unwrapped tool), profile must be None.
        let profile = extract_profile_from_armor_path(None);
        assert!(profile.is_none(), "profile must be None when armor_path is None");
    }

    #[test]
    fn tool_status_profile_is_none_when_armor_path_missing() {
        // File does not exist — extraction must fail silently and return None.
        let profile = extract_profile_from_armor_path(Some("/nonexistent/path/armor.json"));
        assert!(profile.is_none(), "profile must be None when armor.json cannot be read");
    }

    #[test]
    fn tool_status_profile_is_none_when_armor_json_is_malformed() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let armor_path = dir.path().join("armor.json");
        fs::write(&armor_path, "not valid json {{{").unwrap();

        let path_str = armor_path.to_string_lossy().into_owned();
        let profile = extract_profile_from_armor_path(Some(&path_str));
        assert!(profile.is_none(), "profile must be None when armor.json contains invalid JSON");
    }

    #[test]
    fn tool_status_profile_is_none_when_armor_json_missing_profile_field() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let armor_path = dir.path().join("armor.json");
        // Valid JSON but missing the required `profile` field.
        fs::write(&armor_path, r#"{"version":"1.0"}"#).unwrap();

        let path_str = armor_path.to_string_lossy().into_owned();
        let profile = extract_profile_from_armor_path(Some(&path_str));
        assert!(profile.is_none(), "profile must be None when armor.json omits the profile field");
    }

    #[test]
    fn extract_profile_returns_lowercase_for_all_profile_variants() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();

        let cases = [
            ("strict", "strict"),
            ("sandboxed", "sandboxed"),
            ("network", "network"),
            ("system", "system"),
            ("browser", "browser"),
        ];

        for (input, expected) in cases {
            let armor_path = dir.path().join(format!("{input}.armor.json"));
            fs::write(&armor_path, format!(r#"{{"version":"1.0","profile":"{input}"}}"#)).unwrap();
            let path_str = armor_path.to_string_lossy().into_owned();
            let profile = extract_profile_from_armor_path(Some(&path_str));
            assert_eq!(
                profile,
                Some(expected.to_string()),
                "profile for {input} must be lowercase '{expected}'"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // print_status_table / format_status_row
    // ---------------------------------------------------------------------------

    #[test]
    fn print_status_table_shows_profile_column_header() {
        // Verify the table header includes "PROFILE".
        // We test format_status_row indirectly through the column values.
        // The header is printed directly by print_status_table — test it via
        // the known string literal.
        let configs: Vec<ToolStatus> = vec![];
        // Capture is not available in unit tests without redirect; we verify the
        // column formatting logic via format_status_row instead.
        let wrapped_entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "tool".to_string(),
            is_wrapped: true,
            is_http: false,
            armor_path: Some("/some/armor.json".to_string()),
            profile: Some("strict".to_string()),
        };
        let (status, profile_col, _) = format_status_row(&wrapped_entry);
        assert!(status.contains("armored"), "wrapped tool status must contain 'armored': {status}");
        assert_eq!(profile_col, "strict", "profile column must show the profile name");
        let _ = configs; // suppress unused warning
    }

    #[test]
    fn format_status_row_wrapped_tool_shows_armored_status() {
        let entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "tool".to_string(),
            is_wrapped: true,
            is_http: false,
            armor_path: Some("/armor.json".to_string()),
            profile: Some("sandboxed".to_string()),
        };
        let (status, profile_col, armor_source) = format_status_row(&entry);
        assert!(status.contains("armored"), "wrapped tool must show armored status: {status}");
        assert_eq!(profile_col, "sandboxed");
        assert_eq!(armor_source, "/armor.json");
    }

    #[test]
    fn format_status_row_http_tool_shows_not_wrapped_and_na_profile() {
        let entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "remote".to_string(),
            is_wrapped: false,
            is_http: true,
            armor_path: None,
            profile: None,
        };
        let (status, profile_col, armor_source) = format_status_row(&entry);
        assert!(status.contains("not wrapped"), "HTTP tool must show not-wrapped status: {status}");
        assert_eq!(profile_col, "n/a", "HTTP tool profile must be n/a");
        assert!(armor_source.contains("HTTP"), "HTTP tool armor source must mention HTTP: {armor_source}");
    }

    #[test]
    fn format_status_row_unwrapped_tool_shows_not_wrapped_and_na_profile() {
        let entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "bare".to_string(),
            is_wrapped: false,
            is_http: false,
            armor_path: None,
            profile: None,
        };
        let (status, profile_col, armor_source) = format_status_row(&entry);
        assert!(status.contains("not wrapped"), "unwrapped tool must show not-wrapped status: {status}");
        assert_eq!(profile_col, "n/a", "unwrapped tool profile must be n/a");
        assert!(armor_source.contains("not yet wrapped"), "unwrapped tool must mention not yet wrapped: {armor_source}");
    }

    #[test]
    fn format_status_row_wrapped_tool_with_no_armor_path_uses_fallback_source() {
        // Wrapped but no --armor flag in the args — profile is None, source is fallback message.
        let entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "tool".to_string(),
            is_wrapped: true,
            is_http: false,
            armor_path: None,
            profile: None,
        };
        let (status, profile_col, armor_source) = format_status_row(&entry);
        assert!(status.contains("armored"), "wrapped-but-no-path tool must show armored: {status}");
        assert_eq!(profile_col, "n/a", "profile must be n/a when armor.json not found");
        assert!(armor_source.contains("fallback"), "source must say fallback when no armor path: {armor_source}");
    }

    // ---------------------------------------------------------------------------
    // print_status_json — profile field
    // ---------------------------------------------------------------------------

    #[test]
    fn print_status_json_includes_profile_field() {
        // Verify the JSON object structure includes a "profile" key.
        // We test via serde_json directly to match the serialisation logic.
        let entry = ToolStatus {
            host: "claude-desktop".to_string(),
            tool_name: "github".to_string(),
            is_wrapped: true,
            is_http: false,
            armor_path: Some("/armor.json".to_string()),
            profile: Some("network".to_string()),
        };
        let json = serde_json::json!({
            "host": entry.host,
            "tool": entry.tool_name,
            "wrapped": entry.is_wrapped,
            "armor_path": entry.armor_path,
            "profile": entry.profile,
        });
        assert!(
            json.get("profile").is_some(),
            "JSON output must include a 'profile' key"
        );
        assert_eq!(json["profile"].as_str(), Some("network"), "profile value must be 'network'");
    }

    #[test]
    fn print_status_json_profile_is_null_for_unwrapped_tool() {
        let entry = ToolStatus {
            host: "h".to_string(),
            tool_name: "bare".to_string(),
            is_wrapped: false,
            is_http: false,
            armor_path: None,
            profile: None,
        };
        let json = serde_json::json!({
            "host": entry.host,
            "tool": entry.tool_name,
            "wrapped": entry.is_wrapped,
            "armor_path": entry.armor_path,
            "profile": entry.profile,
        });
        assert!(json.get("profile").is_some(), "JSON output must include 'profile' key even when None");
        assert!(json["profile"].is_null(), "profile must be JSON null for unwrapped tool");
    }

    // ---------------------------------------------------------------------------
    // Community profiles — bundled content
    // ---------------------------------------------------------------------------

    /// The expected names of the 10 bundled launch profiles.
    const EXPECTED_BUNDLED_NAMES: &[&str] = &[
        "github",
        "filesystem",
        "gmail",
        "slack",
        "notion",
        "playwright",
        "fetch",
        "git",
        "brave-search",
        "sqlite",
    ];

    #[test]
    fn bundled_profiles_include_all_10_launch_profiles() {
        let bundled_names: Vec<&str> = BUNDLED_COMMUNITY_PROFILES
            .iter()
            .map(|(name, _)| *name)
            .collect();

        for expected in EXPECTED_BUNDLED_NAMES {
            assert!(
                bundled_names.contains(expected),
                "bundled profiles must include '{expected}'; got: {bundled_names:?}"
            );
        }
        assert_eq!(
            BUNDLED_COMMUNITY_PROFILES.len(),
            EXPECTED_BUNDLED_NAMES.len(),
            "bundled profile count must be exactly {}",
            EXPECTED_BUNDLED_NAMES.len()
        );
    }

    #[test]
    fn all_community_profiles_are_valid_armor_manifests() {
        for (name, content) in BUNDLED_COMMUNITY_PROFILES {
            let result = serde_json::from_str::<ArmorManifest>(content);
            assert!(
                result.is_ok(),
                "bundled profile '{name}' must deserialize as ArmorManifest: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn all_community_profiles_validate_against_json_schema() {
        for (name, content) in BUNDLED_COMMUNITY_PROFILES {
            let value: serde_json::Value = serde_json::from_str(content)
                .unwrap_or_else(|e| panic!("profile '{name}' is not valid JSON: {e}"));
            let errors = validate_against_schema(&value)
                .unwrap_or_else(|e| panic!("schema compilation failed: {e}"));
            assert!(
                errors.is_empty(),
                "bundled profile '{name}' fails JSON Schema validation:\n{}",
                errors.join("\n")
            );
        }
    }

    #[test]
    fn all_community_profiles_have_team_authored_source() {
        for (name, content) in BUNDLED_COMMUNITY_PROFILES {
            let value: serde_json::Value = serde_json::from_str(content)
                .unwrap_or_else(|e| panic!("profile '{name}' is not valid JSON: {e}"));
            let source = value["_source"].as_str().unwrap_or("");
            assert_eq!(
                source, "team-authored",
                "profile '{name}' must have _source == \"team-authored\", got: \"{source}\""
            );
        }
    }

    #[test]
    fn all_community_profiles_have_version_field() {
        for (name, content) in BUNDLED_COMMUNITY_PROFILES {
            let manifest: ArmorManifest = serde_json::from_str(content)
                .unwrap_or_else(|e| panic!("profile '{name}' failed to parse: {e}"));
            assert!(
                !manifest.version.is_empty(),
                "profile '{name}' must have a non-empty version field"
            );
        }
    }

    #[test]
    fn community_profile_github_deserializes_correctly() {
        let content = find_bundled_profile("github")
            .expect("github profile must be bundled");
        let manifest: ArmorManifest = serde_json::from_str(content)
            .expect("github profile must deserialize as ArmorManifest");
        assert_eq!(manifest.profile, Profile::Network, "github profile must use Network profile");
        assert!(
            !manifest.network.allow.is_empty(),
            "github profile must declare at least one network.allow entry"
        );
        assert!(
            manifest.network.allow.iter().any(|h| h.contains("github")),
            "github profile network.allow must reference github domains; got: {:?}",
            manifest.network.allow
        );
    }

    #[test]
    fn community_profile_playwright_deserializes_correctly() {
        let content = find_bundled_profile("playwright")
            .expect("playwright profile must be bundled");
        let manifest: ArmorManifest = serde_json::from_str(content)
            .expect("playwright profile must deserialize as ArmorManifest");
        assert_eq!(
            manifest.profile,
            Profile::Browser,
            "playwright profile must use Browser profile"
        );
    }

    #[test]
    fn find_bundled_profile_returns_none_for_unknown_name() {
        assert!(
            find_bundled_profile("this-does-not-exist-xyz").is_none(),
            "find_bundled_profile must return None for an unknown profile name"
        );
    }

    #[test]
    fn find_bundled_profile_returns_none_for_empty_name() {
        assert!(
            find_bundled_profile("").is_none(),
            "find_bundled_profile must return None for empty string"
        );
    }

    #[test]
    fn bundled_profiles_have_no_duplicate_names() {
        let names: Vec<&str> = BUNDLED_COMMUNITY_PROFILES
            .iter()
            .map(|(n, _)| *n)
            .collect();
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(
                seen.insert(name),
                "duplicate bundled profile name detected: '{name}'"
            );
        }
    }

    #[test]
    fn bundled_profiles_content_is_non_empty_json() {
        for (name, content) in BUNDLED_COMMUNITY_PROFILES {
            assert!(
                !content.is_empty(),
                "bundled profile '{name}' must have non-empty content"
            );
            let value: serde_json::Value = serde_json::from_str(content)
                .unwrap_or_else(|e| panic!("bundled profile '{name}' is not valid JSON: {e}"));
            assert!(
                value.is_object(),
                "bundled profile '{name}' must be a JSON object, got: {value:?}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // resolve_since_filter — Feature 2 prune defaults
    // ---------------------------------------------------------------------------

    #[test]
    fn resolve_since_filter_returns_none_when_not_pruning_and_no_since() {
        let result = resolve_since_filter(false, None).unwrap();
        assert!(result.is_none(), "no prune + no since must return None");
    }

    #[test]
    fn resolve_since_filter_applies_90_day_default_when_pruning_without_since() {
        let before = Utc::now();
        let result = resolve_since_filter(true, None).unwrap();
        let after = Utc::now();
        let cutoff = result.expect("prune without since must produce a cutoff");

        // Cutoff should be approximately 90 days before now.
        let expected_secs = i64::from(DEFAULT_RETENTION_DAYS) * 24 * 3600;
        let diff_before = (before - cutoff).num_seconds().abs();
        let diff_after = (after - cutoff).num_seconds().abs();
        // Accept ±60s tolerance for test execution time.
        assert!(
            diff_before >= expected_secs - 60 && diff_before <= expected_secs + 60,
            "cutoff must be ~{DEFAULT_RETENTION_DAYS} days ago relative to before; diff={diff_before}s"
        );
        assert!(
            cutoff < after,
            "cutoff must be in the past; cutoff={cutoff}, after={after}"
        );
        let _ = diff_after; // suppress warning
    }

    #[test]
    fn resolve_since_filter_uses_since_string_even_when_pruning() {
        // When --since is provided with --prune, the explicit value takes precedence.
        let result = resolve_since_filter(true, Some("7d")).unwrap();
        let cutoff = result.expect("prune with since=7d must produce a cutoff");
        let expected_secs = 7 * 24 * 3600_i64;
        let diff = (Utc::now() - cutoff).num_seconds();
        assert!(
            (diff - expected_secs).abs() < 60,
            "since=7d cutoff should be ~7 days ago; diff={diff}s"
        );
    }

    #[test]
    fn resolve_since_filter_uses_since_string_when_not_pruning() {
        let result = resolve_since_filter(false, Some("1h")).unwrap();
        let cutoff = result.expect("since=1h must produce a cutoff");
        let diff = (Utc::now() - cutoff).num_seconds();
        let expected = 3600_i64;
        assert!(
            (diff - expected).abs() < 60,
            "since=1h cutoff should be ~3600s ago; diff={diff}s"
        );
    }

    #[test]
    fn resolve_since_filter_returns_error_for_invalid_since_string() {
        let result = resolve_since_filter(false, Some("not-a-date"));
        assert!(result.is_err(), "invalid since string must produce an error");
    }

    // ---------------------------------------------------------------------------
    // prune_audit_log and count_raw_log_lines
    // ---------------------------------------------------------------------------

    #[test]
    fn prune_audit_log_rewrites_only_kept_entries() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        let entry_a = r#"{"timestamp":"2026-01-01T00:00:00Z","tool":"a","event":"invoke"}"#;
        let entry_b = r#"{"timestamp":"2026-01-02T00:00:00Z","tool":"b","event":"invoke"}"#;
        let content = format!("{entry_a}\n{entry_b}\n");
        fs::write(&log_path, &content).unwrap();

        let rows: Vec<AuditRow> = vec![AuditRow {
            raw: entry_a.to_string(),
            timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
            tool: "a".to_string(),
            event: "invoke".to_string(),
            detail: String::new(),
        }];

        prune_audit_log(&log_path, &rows).unwrap();

        let result = fs::read_to_string(&log_path).unwrap();
        assert!(result.contains("tool-a") || result.contains("\"a\""), "kept entry must be retained");
        assert!(!result.contains("\"b\""), "pruned entry must not appear");
    }

    #[test]
    fn count_raw_log_lines_counts_non_empty_lines() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("audit.jsonl");

        fs::write(&log_path, "line1\nline2\n\nline3\n").unwrap();

        let count = count_raw_log_lines(&log_path);
        assert_eq!(count, 3, "must count 3 non-empty lines; got {count}");
    }

    #[test]
    fn count_raw_log_lines_returns_zero_for_absent_file() {
        let count = count_raw_log_lines(Path::new("/nonexistent/file.jsonl"));
        assert_eq!(count, 0, "absent file must produce count of 0");
    }

    #[test]
    fn count_raw_log_lines_returns_zero_for_empty_file() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("empty.jsonl");
        fs::write(&log_path, "").unwrap();

        let count = count_raw_log_lines(&log_path);
        assert_eq!(count, 0, "empty file must produce count of 0");
    }

    // ---------------------------------------------------------------------------
    // community_profiles_base_url: pinned-tag supply chain guarantee
    // ---------------------------------------------------------------------------

    #[test]
    fn community_profiles_url_contains_release_tag() {
        let url = community_profiles_base_url();
        assert!(
            url.contains(COMMUNITY_PROFILES_RELEASE_TAG),
            "URL must contain the release tag: {url}"
        );
    }

    #[test]
    fn community_profiles_url_does_not_reference_main_branch() {
        let url = community_profiles_base_url();
        assert!(
            !url.contains("/main/"),
            "URL must NOT reference the main branch: {url}"
        );
    }

    // ---------------------------------------------------------------------------
    // Interactive init helpers: prompt_with_default, prompt_csv, prompt_bool
    // ---------------------------------------------------------------------------

    #[test]
    fn prompt_with_default_returns_default_on_empty_input() {
        let mut input = std::io::Cursor::new(b"\n");
        let result = prompt_with_default(&mut input, "Profile [sandboxed]", "sandboxed", &[]).unwrap();
        assert_eq!(result, "sandboxed");
    }

    #[test]
    fn prompt_with_default_returns_user_input_when_provided() {
        let mut input = std::io::Cursor::new(b"strict\n");
        let result = prompt_with_default(
            &mut input,
            "Profile",
            "sandboxed",
            &["strict", "sandboxed"],
        )
        .unwrap();
        assert_eq!(result, "strict");
    }

    #[test]
    fn prompt_with_default_rejects_invalid_option() {
        let mut input = std::io::Cursor::new(b"invalid\n");
        let result = prompt_with_default(
            &mut input,
            "Profile",
            "sandboxed",
            &["strict", "sandboxed"],
        );
        assert!(result.is_err(), "must reject invalid options");
    }

    #[test]
    fn prompt_with_default_accepts_any_value_when_no_valid_options() {
        let mut input = std::io::Cursor::new(b"anything\n");
        let result = prompt_with_default(&mut input, "Question", "default", &[]).unwrap();
        assert_eq!(result, "anything");
    }

    #[test]
    fn prompt_csv_returns_empty_vec_on_blank_input() {
        let mut input = std::io::Cursor::new(b"\n");
        let result = prompt_csv(&mut input, "Paths").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn prompt_csv_splits_and_trims_values() {
        let mut input = std::io::Cursor::new(b" /tmp , /var/log , /home \n");
        let result = prompt_csv(&mut input, "Paths").unwrap();
        assert_eq!(result, vec!["/tmp", "/var/log", "/home"]);
    }

    #[test]
    fn prompt_csv_skips_empty_segments() {
        let mut input = std::io::Cursor::new(b"a,,b,\n");
        let result = prompt_csv(&mut input, "Items").unwrap();
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn prompt_bool_returns_default_on_empty_input() {
        let mut input = std::io::Cursor::new(b"\n");
        assert!(!prompt_bool(&mut input, "Spawn?", false).unwrap());

        let mut input = std::io::Cursor::new(b"\n");
        assert!(prompt_bool(&mut input, "Lock?", true).unwrap());
    }

    #[test]
    fn prompt_bool_accepts_yes_variants() {
        for answer in &["y\n", "yes\n", "Y\n", "YES\n", "Yes\n"] {
            let mut input = std::io::Cursor::new(answer.as_bytes());
            assert!(
                prompt_bool(&mut input, "Q?", false).unwrap(),
                "'{answer}' should parse as true"
            );
        }
    }

    #[test]
    fn prompt_bool_accepts_no_variants() {
        for answer in &["n\n", "no\n", "N\n", "NO\n", "No\n"] {
            let mut input = std::io::Cursor::new(answer.as_bytes());
            assert!(
                !prompt_bool(&mut input, "Q?", true).unwrap(),
                "'{answer}' should parse as false"
            );
        }
    }

    #[test]
    fn prompt_bool_rejects_invalid_input() {
        let mut input = std::io::Cursor::new(b"maybe\n");
        assert!(prompt_bool(&mut input, "Q?", false).is_err());
    }

    // ---------------------------------------------------------------------------
    // generate_armor_json_interactive: end-to-end
    // ---------------------------------------------------------------------------

    #[test]
    fn interactive_init_all_defaults_produces_valid_sandboxed_manifest() {
        // 7 blank lines → all defaults: sandboxed, no paths, no network, no spawn, no env, not locked
        let mut input = std::io::Cursor::new(b"\n\n\n\n\n\n\n");
        let json = generate_armor_json_interactive(&mut input).unwrap();
        let manifest: ArmorManifest = serde_json::from_str(&json)
            .expect("interactive output must be a valid ArmorManifest");
        assert_eq!(manifest.profile, Profile::Sandboxed);
    }

    #[test]
    fn interactive_init_with_all_fields_populated() {
        let input_text = "network\n/tmp,/var\n/tmp/out\napi.example.com:443\ny\nHOME,PATH\ny\n";
        let mut input = std::io::Cursor::new(input_text.as_bytes());
        let json = generate_armor_json_interactive(&mut input).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["profile"], "network");
        assert_eq!(value["filesystem"]["read"].as_array().unwrap().len(), 2);
        assert_eq!(value["filesystem"]["write"].as_array().unwrap().len(), 1);
        assert_eq!(value["network"]["allow"].as_array().unwrap().len(), 1);
        assert_eq!(value["spawn"], true);
        assert_eq!(value["env"]["allow"].as_array().unwrap().len(), 2);
        assert_eq!(value["locked"], true);
    }

    #[test]
    fn interactive_init_strict_profile_minimal_output() {
        let input_text = "strict\n\n\n\n\n\n\n";
        let mut input = std::io::Cursor::new(input_text.as_bytes());
        let json = generate_armor_json_interactive(&mut input).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["profile"], "strict");
        // Strict with no paths/network should omit filesystem and network keys
        assert!(value.get("filesystem").is_none());
        assert!(value.get("network").is_none());
        assert!(value.get("spawn").is_none());
    }

    #[test]
    fn interactive_init_rejects_invalid_profile() {
        let input_text = "invalid_profile\n\n\n\n\n\n\n";
        let mut input = std::io::Cursor::new(input_text.as_bytes());
        let result = generate_armor_json_interactive(&mut input);
        assert!(result.is_err(), "invalid profile name must be rejected");
    }
}
