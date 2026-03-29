//! Stdio proxy loop — the core of the M1 broker.
//!
//! Reads JSON-RPC messages from the host on stdin, inspects tool calls,
//! forwards allowed messages to the tool subprocess, and returns responses
//! (optionally after secret scanning) back to the host on stdout.

use anyhow::{Context, Result};
use mcparmor_core::audit::AuditEntry;
use mcparmor_core::errors::BrokerError;
use mcparmor_core::manifest::{ArmorManifest, SecretScanMode};
use mcparmor_core::scanner;
use serde_json::Value;
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[cfg(unix)]
use libc;

use crate::audit_writer::AuditWriter;
use crate::inspect::{self, InspectResult};
use crate::sandbox::SandboxProvider;

/// Seconds after spawn within which a non-zero exit is treated as a startup failure.
const EARLY_EXIT_WINDOW_SECS: u64 = 3;

/// Shell exit code meaning "command not found" (POSIX standard).
const EXIT_CODE_COMMAND_NOT_FOUND: i32 = 127;

/// Seconds to wait for graceful SIGTERM before sending SIGKILL on timeout.
const SIGTERM_GRACE_PERIOD_SECS: u64 = 2;

/// Suffix appended to tool descriptions when annotation is enabled.
const ARMOR_ANNOTATION: &str = " [🛡 MCP Armor]";

/// Shared configuration for the host-to-tool and tool-to-host forwarding tasks.
///
/// Bundles the manifest, audit writer, and logging flags so they can be passed
/// as a single argument instead of three separate parameters.
#[derive(Clone)]
struct ForwardConfig {
    /// Parsed armor manifest for the tool.
    manifest: Arc<ArmorManifest>,
    /// Audit log writer.
    audit_writer: Arc<AuditWriter>,
    /// When true, parameter values are omitted from audit log entries (keys only).
    no_log_params: bool,
    /// When true, any capability violation immediately terminates the session with exit code 2.
    strict_mode: bool,
    /// When true, prints allow/deny decisions to stderr for each message.
    verbose: bool,
    /// When true, annotates tool descriptions in `tools/list` responses with a shield indicator.
    annotate: bool,
    /// Request IDs of `tools/list` messages, used to identify matching responses.
    tools_list_ids: Arc<Mutex<HashSet<String>>>,
}

/// Configuration for the stdio proxy loop.
pub struct ProxyConfig {
    /// Parsed armor manifest for the tool.
    pub manifest: Arc<ArmorManifest>,
    /// Sandbox provider (Noop, Linux, etc.) selected for this platform.
    pub sandbox: Arc<dyn SandboxProvider>,
    /// Audit log writer.
    pub audit_writer: Arc<AuditWriter>,
    /// When true, OS sandbox is bypassed (--no-os-sandbox flag).
    // Only read in platform-specific code paths (e.g. Linux configure_pre_exec).
    #[allow(dead_code)]
    pub no_os_sandbox: bool,
    /// When true, parameter values are omitted from audit log entries (keys only).
    pub no_log_params: bool,
    /// When true, any capability violation immediately terminates the session with exit code 2.
    pub strict_mode: bool,
    /// When true, prints allow/deny decisions to stderr for each message.
    pub verbose: bool,
    /// Display name of the tool (derived from argv[0]) used in audit log entries.
    pub tool_name: String,
    /// When true, annotates tool descriptions in `tools/list` responses with a
    /// shield indicator so the host UI shows which tools are protected.
    pub annotate: bool,
}

/// Run the stdio proxy for the given tool command under the manifest policy.
///
/// Spawns the tool as a child process, then concurrently:
/// - Forwards host stdin → tool stdin (with Layer 1 inspection)
/// - Forwards tool stdout → host stdout (with secret scanning)
/// - Pipes tool stderr → broker stderr (transparent pass-through)
///
/// If `manifest.timeout_ms` is set, the entire session is subject to that
/// wall-clock limit. Exceeding it sends SIGKILL to the tool process group
/// and returns a timeout error to the host.
///
/// # Arguments
/// * `config` - Proxy configuration (manifest, sandbox, audit writer, flags)
/// * `tool_command` - Full command line for the tool (argv[0] + args)
///
/// # Errors
/// Returns an error if the tool cannot be spawned or if a fatal I/O error occurs.
pub async fn run_proxy(config: ProxyConfig, tool_command: &[String]) -> Result<()> {
    let (program, args) = split_command(tool_command)?;
    let sandboxed = config.sandbox.apply(&config.manifest, program, args)?;

    print_startup_banner(&config);

    let (mut child, spawn_time) = spawn_tool_process(&config, &sandboxed)?;

    let child_stdin = child.stdin.take().context("Child stdin unavailable")?;
    let child_stdout = child.stdout.take().context("Child stdout unavailable")?;

    // Shared flag: set to true when the first JSON-RPC message is forwarded to
    // the tool. Used in exit diagnostics to detect premature exits.
    let first_message_sent = Arc::new(AtomicBool::new(false));
    let forward_config = make_forward_config(&config);

    let forward_handle = tokio::spawn(forward_host_to_tool(
        forward_config.clone(),
        child_stdin,
        first_message_sent.clone(),
    ));

    let return_handle = tokio::spawn(forward_tool_to_host(
        forward_config,
        child_stdout,
        config.tool_name.clone(),
    ));

    run_until_complete_or_timeout(
        config.manifest.timeout_ms,
        &mut child,
        forward_handle,
        return_handle,
        spawn_time,
        &first_message_sent,
    )
    .await
}

/// Spawn the tool subprocess and return it with the instant it was started.
///
/// Applies Linux pre-exec hooks (Landlock) when the OS sandbox is enabled.
fn spawn_tool_process(
    config: &ProxyConfig,
    sandboxed: &crate::sandbox::SandboxedCommand,
) -> Result<(tokio::process::Child, Instant)> {
    let mut cmd = build_command(&sandboxed.program, &sandboxed.args, &config.manifest.env.allow);

    #[cfg(target_os = "linux")]
    if !config.no_os_sandbox {
        crate::sandbox::linux::configure_pre_exec(config.manifest.clone(), cmd.as_std_mut());
    }

    let spawn_time = Instant::now();
    let child = cmd.spawn().context("Failed to spawn tool subprocess")?;
    Ok((child, spawn_time))
}

/// Build a `ForwardConfig` from the proxy config for use in both forwarding tasks.
fn make_forward_config(config: &ProxyConfig) -> ForwardConfig {
    ForwardConfig {
        manifest: config.manifest.clone(),
        audit_writer: config.audit_writer.clone(),
        no_log_params: config.no_log_params,
        strict_mode: config.strict_mode,
        verbose: config.verbose,
        annotate: config.annotate,
        tools_list_ids: Arc::new(Mutex::new(HashSet::new())),
    }
}

/// Wait for the forwarding tasks and child process to complete, applying a timeout if set.
async fn run_until_complete_or_timeout(
    timeout_ms: Option<u32>,
    child: &mut tokio::process::Child,
    forward_handle: tokio::task::JoinHandle<()>,
    return_handle: tokio::task::JoinHandle<()>,
    spawn_time: Instant,
    first_message_sent: &AtomicBool,
) -> Result<()> {
    match timeout_ms {
        Some(ms) => {
            let duration = Duration::from_millis(u64::from(ms));
            run_with_timeout(duration, ms, child, forward_handle, return_handle).await
        }
        None => {
            await_forwarding_tasks(forward_handle, return_handle).await;
            let status = child.wait().await.context("Failed to wait for child process")?;
            diagnose_early_exit(status, spawn_time, first_message_sent);
            Ok(())
        }
    }
}

/// Await both forwarding tasks, logging any join errors.
async fn await_forwarding_tasks(
    forward_handle: tokio::task::JoinHandle<()>,
    return_handle: tokio::task::JoinHandle<()>,
) {
    if let Err(e) = forward_handle.await {
        tracing::warn!("Host-to-tool forwarding task failed: {e:#}");
    }
    if let Err(e) = return_handle.await {
        tracing::warn!("Tool-to-host forwarding task failed: {e:#}");
    }
}

/// Build a tokio `Command` for the tool with piped stdio and filtered env.
///
/// The child process is placed in its own process group so that a timeout
/// can kill the entire group (including any grandchild processes) via
/// `kill(-pgid, SIGKILL)` rather than only the direct child.
fn build_command(program: &str, args: &[String], env_allow: &[String]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env_clear();

    // Place the child in its own process group (pgid = child pid).
    // This allows kill(-pgid, SIGKILL) to reach all descendants on timeout.
    #[cfg(unix)]
    cmd.process_group(0);

    for key in env_allow {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

    cmd
}

/// Split the tool command slice into program and args.
fn split_command(command: &[String]) -> Result<(&str, &[String])> {
    let (program, args) = command
        .split_first()
        .context("Tool command must not be empty")?;
    Ok((program.as_str(), args))
}

/// Forward host stdin → tool stdin, inspecting each tools/call message.
///
/// Non-tools/call messages are passed through verbatim. Blocked messages
/// receive a JSON-RPC error response written to stdout.
///
/// Sets `first_message_sent` to `true` when the first message is successfully
/// forwarded to the tool's stdin. Used for early-exit diagnostics.
async fn forward_host_to_tool(
    config: ForwardConfig,
    mut tool_stdin: tokio::process::ChildStdin,
    first_message_sent: Arc<AtomicBool>,
) {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        if line.is_empty() {
            continue;
        }

        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            // Malformed JSON — pass through and let the tool handle it.
            if let Err(e) = write_line(&mut tool_stdin, &line).await {
                tracing::warn!("Failed to forward malformed message to tool stdin: {e:#}");
            }
            continue;
        };

        // Track tools/list request IDs so the return path can annotate responses.
        if config.annotate {
            track_tools_list_id(&message, &config.tools_list_ids).await;
        }

        match inspect::check_message(&message, &config.manifest) {
            InspectResult::Allow => {
                if handle_allowed_message(&config, &message, &line, &mut tool_stdin).await {
                    first_message_sent.store(true, Ordering::Relaxed);
                } else {
                    break;
                }
            }
            InspectResult::Deny(err) => {
                handle_denied_message(&config, &message, err).await;
            }
        }
    }
}

/// Handle a message that passed inspection: log, audit, and forward to tool stdin.
///
/// Returns `true` when the message was forwarded successfully, `false` when
/// the write to tool stdin failed (the caller should break the read loop).
async fn handle_allowed_message(
    config: &ForwardConfig,
    message: &Value,
    raw_line: &str,
    tool_stdin: &mut tokio::process::ChildStdin,
) -> bool {
    if config.verbose {
        let method = message.get("method").and_then(|v| v.as_str()).unwrap_or("unknown");
        eprintln!("[mcparmor] ALLOW {method}");
    }
    write_invoke_audit(message, &config.manifest, &config.audit_writer, config.no_log_params);
    if let Err(e) = write_line(tool_stdin, raw_line).await {
        tracing::warn!("Failed to forward message to tool stdin: {e:#}");
        return false;
    }
    true
}

/// Handle a message that was denied: log, audit, and send a JSON-RPC error response.
///
/// Denials are always printed to stderr because they represent security-relevant
/// enforcement events. In strict mode, exits the process with code 2 after
/// sending the error response.
async fn handle_denied_message(config: &ForwardConfig, message: &Value, err: mcparmor_core::errors::BrokerError) {
    let method = message.get("method").and_then(|v| v.as_str()).unwrap_or("unknown");
    eprintln!("[mcparmor] BLOCKED {method} — {}", err.message);
    write_violation_audit(message, &err, &config.manifest, &config.audit_writer);
    let error_response = build_error_response(message, err);
    if let Err(e) = write_json_to_stdout(&error_response).await {
        tracing::warn!("Failed to write violation error response to stdout: {e:#}");
    }
    if config.strict_mode {
        // In strict mode, any capability violation is fatal — exit code 2.
        std::process::exit(2);
    }
}

/// Forward tool stdout → host stdout with secret scanning applied.
async fn forward_tool_to_host(
    config: ForwardConfig,
    tool_stdout: tokio::process::ChildStdout,
    tool_name: String,
) {
    let mut reader = BufReader::new(tool_stdout).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        if line.is_empty() {
            continue;
        }

        let start = Instant::now();

        // Annotate tools/list responses with the MCP Armor shield indicator.
        let line = if config.annotate {
            annotate_tools_list_response(&line, &config.tools_list_ids).await
        } else {
            line
        };

        let processed = process_response(&line, &config.manifest, &config.audit_writer, &tool_name);
        // Cap at u64::MAX (≈584 million years) to avoid wrapping on pathological inputs.
        let latency_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;

        match processed {
            ResponseAction::Forward(text) => {
                write_response_audit(&text, latency_ms, &tool_name, &config.manifest, &config.audit_writer);
                if let Err(e) = write_raw_to_stdout(&text).await {
                    tracing::warn!("Failed to write response to stdout: {e:#}");
                }
            }
            ResponseAction::Block(err) => {
                // Attempt to extract the id from the original line for the error response.
                let id = extract_id_from_line(&line);
                let error_response = build_error_response_with_id(id, err);
                if let Err(e) = write_json_to_stdout(&error_response).await {
                    tracing::warn!("Failed to write block error response to stdout: {e:#}");
                }
            }
        }
    }
}

/// The result of processing a tool response line.
enum ResponseAction {
    /// Forward this text to the host (possibly redacted).
    Forward(String),
    /// Block and send an error response with this error.
    Block(BrokerError),
}

/// Apply size truncation and secret scanning to a raw response line.
fn process_response(
    line: &str,
    manifest: &ArmorManifest,
    audit_writer: &Arc<AuditWriter>,
    tool_name: &str,
) -> ResponseAction {
    let truncated = apply_size_limit(line, &manifest.output.max_size_kb);

    match &manifest.output.scan_secrets {
        SecretScanMode::Disabled => ResponseAction::Forward(truncated.to_string()),
        SecretScanMode::Redact => {
            let result = scanner::scan(&truncated);
            if !result.detections.is_empty() {
                for detection in &result.detections {
                    write_secret_audit(&detection.secret_type, tool_name, manifest, audit_writer);
                }
            }
            ResponseAction::Forward(result.redacted)
        }
        SecretScanMode::Strict => {
            let result = scanner::scan(&truncated);
            if let Some(first) = result.detections.first() {
                write_secret_audit(&first.secret_type, tool_name, manifest, audit_writer);
                ResponseAction::Block(BrokerError::secret_detected(&first.secret_type))
            } else {
                ResponseAction::Forward(truncated.to_string())
            }
        }
    }
}

/// Truncate a response string to the configured max size (in KB).
///
/// Returns a slice of the original if under the limit, or a truncated owned string.
fn apply_size_limit<'a>(line: &'a str, max_size_kb: &Option<u32>) -> std::borrow::Cow<'a, str> {
    let Some(limit_kb) = max_size_kb else {
        return std::borrow::Cow::Borrowed(line);
    };
    let limit_bytes = usize::try_from(*limit_kb).unwrap_or(usize::MAX) * 1024;
    if line.len() <= limit_bytes {
        return std::borrow::Cow::Borrowed(line);
    }
    // Truncate at a valid UTF-8 boundary.
    let truncated = truncate_utf8(line, limit_bytes);
    std::borrow::Cow::Owned(truncated.to_string())
}

/// Truncate a str to at most `max_bytes` bytes at a valid UTF-8 character boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Build a JSON-RPC error response preserving the request's `id` field.
fn build_error_response(request: &Value, err: BrokerError) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    build_error_response_with_id(id, err)
}

/// Build a JSON-RPC error response with an explicit `id` value.
fn build_error_response_with_id(id: Value, err: BrokerError) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": err.code,
            "message": err.message,
            "data": { "hint": err.hint }
        }
    })
}

/// Extract the JSON-RPC `id` from a raw JSON line without full deserialization.
fn extract_id_from_line(line: &str) -> Value {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(Value::Null)
}

/// Write a line to the tool's stdin.
async fn write_line(stdin: &mut tokio::process::ChildStdin, line: &str) -> Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    Ok(())
}

/// Write a JSON value as a line to the host's stdout.
async fn write_json_to_stdout(value: &Value) -> Result<()> {
    let line = serde_json::to_string(value)?;
    write_raw_to_stdout(&line).await
}

/// Write a raw string as a line to the host's stdout.
async fn write_raw_to_stdout(line: &str) -> Result<()> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(line.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

/// Write an invoke audit entry if auditing is enabled.
///
/// When `no_log_params` is false (default), the entry detail includes the
/// argument keys and values. When true, only argument keys are logged —
/// values are omitted to protect potentially sensitive parameter data.
fn write_invoke_audit(
    message: &Value,
    manifest: &ArmorManifest,
    audit_writer: &Arc<AuditWriter>,
    no_log_params: bool,
) {
    if !manifest.audit.enabled {
        return;
    }
    let tool_name = extract_tool_name(message);
    let detail = build_invoke_detail(message, no_log_params);
    let entry = AuditEntry::invoke(&tool_name, detail);
    if let Err(e) = audit_writer.write(&entry) {
        tracing::warn!("Failed to write invoke audit entry: {e:#}");
    }
}

/// Build the detail string for an invoke audit entry.
///
/// Returns a JSON-serialized object with argument keys and (optionally) values.
fn build_invoke_detail(message: &Value, no_log_params: bool) -> String {
    let arguments = message
        .get("params")
        .and_then(|p| p.get("arguments"))
        .and_then(Value::as_object);

    let Some(args) = arguments else {
        return "tools/call".to_string();
    };

    if no_log_params {
        // Log only the argument key names — omit values.
        let keys: Vec<&str> = args.keys().map(String::as_str).collect();
        format!("keys=[{}]", keys.join(","))
    } else {
        // Log the full arguments object.
        serde_json::to_string(args).unwrap_or_else(|_| "tools/call".to_string())
    }
}

/// Write a param-violation audit entry if auditing is enabled.
fn write_violation_audit(
    message: &Value,
    err: &BrokerError,
    manifest: &ArmorManifest,
    audit_writer: &Arc<AuditWriter>,
) {
    if !manifest.audit.enabled {
        return;
    }
    let tool_name = extract_tool_name(message);
    // The violation message already describes the policy breach without including
    // the raw parameter value, so it is safe to log regardless of no_log_params.
    let entry = AuditEntry::param_violation(&tool_name, &err.message);
    if let Err(e) = audit_writer.write(&entry) {
        tracing::warn!("Failed to write violation audit entry: {e:#}");
    }
}

/// Write a response audit entry if auditing is enabled.
fn write_response_audit(
    line: &str,
    latency_ms: u64,
    tool_name: &str,
    manifest: &ArmorManifest,
    audit_writer: &Arc<AuditWriter>,
) {
    if !manifest.audit.enabled {
        return;
    }
    let entry = AuditEntry::response(tool_name, line.len(), latency_ms);
    if let Err(e) = audit_writer.write(&entry) {
        tracing::warn!("Failed to write response audit entry: {e:#}");
    }
}

/// Write a secret-detected audit entry if auditing is enabled.
fn write_secret_audit(
    secret_type: &str,
    tool_name: &str,
    manifest: &ArmorManifest,
    audit_writer: &Arc<AuditWriter>,
) {
    if !manifest.audit.enabled {
        return;
    }
    let entry = AuditEntry::secret_detected(tool_name, secret_type);
    if let Err(e) = audit_writer.write(&entry) {
        tracing::warn!("Failed to write secret-detected audit entry: {e:#}");
    }
}

/// Extract the tool name from a tools/call message's `params.name` field.
fn extract_tool_name(message: &Value) -> String {
    message
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

/// Emit actionable diagnostics when a tool exits early with a non-zero code.
///
/// "Early" means within 3 seconds of spawn, before the tool has had a chance to
/// process any JSON-RPC messages. This is the signature of a startup failure
/// (missing runtime, missing env vars, bad configuration).
///
/// Diagnostics are written to stderr so they are visible to the operator without
/// polluting the JSON-RPC stdout channel.
fn diagnose_early_exit(
    status: std::process::ExitStatus,
    spawn_time: Instant,
    first_message_sent: &AtomicBool,
) {
    if status.success() {
        return;
    }
    // Only diagnose startup failures — if the tool ran beyond the window it started OK.
    if spawn_time.elapsed() > Duration::from_secs(EARLY_EXIT_WINDOW_SECS) {
        return;
    }

    let exit_code = status.code();

    if exit_code == Some(EXIT_CODE_COMMAND_NOT_FOUND) {
        // Code 127 = command not found (shell or process can't locate a binary).
        eprintln!(
            "[mcparmor] tool exited with code 127 (command not found) — \
             add \"PATH\" to env.allow in armor.json so the tool can locate its runtime."
        );
    } else if !first_message_sent.load(Ordering::Relaxed) {
        // Non-zero exit before the first JSON-RPC message: the tool failed to start.
        eprintln!(
            "[mcparmor] tool exited (code {:?}) before the first JSON-RPC message — \
             if the tool needs HOME or PATH to start, add them to env.allow in armor.json.",
            exit_code
        );
    }
}

/// Run the proxy tasks with a wall-clock timeout.
///
/// On timeout, sends SIGKILL to the child process and writes a timeout error
/// response to the host stdout.
async fn run_with_timeout(
    duration: Duration,
    timeout_ms: u32,
    child: &mut tokio::process::Child,
    forward_handle: tokio::task::JoinHandle<()>,
    return_handle: tokio::task::JoinHandle<()>,
) -> Result<()> {
    let result = tokio::time::timeout(duration, async {
        if let Err(e) = forward_handle.await {
            tracing::warn!("Host-to-tool forwarding task failed: {e:#}");
        }
        if let Err(e) = return_handle.await {
            tracing::warn!("Tool-to-host forwarding task failed: {e:#}");
        }
    })
    .await;

    match result {
        Ok(()) => {
            child.wait().await.context("Failed to wait for child process")?;
            Ok(())
        }
        Err(_elapsed) => {
            kill_child_gracefully(child).await;
            let error_response = build_error_response_with_id(
                Value::Null,
                BrokerError::timeout(timeout_ms),
            );
            write_json_to_stdout(&error_response).await?;
            anyhow::bail!("Tool timed out after {}ms", timeout_ms);
        }
    }
}

/// Send SIGTERM to the child process group, then SIGKILL after the grace period.
///
/// On Unix, signals the entire process group (negative pgid) so that grandchild
/// processes are also terminated. The two-step sequence gives the tool a chance
/// to flush buffers and clean up before the hard kill.
///
/// Falls back to `child.kill()` (SIGKILL only) on non-Unix platforms.
async fn kill_child_gracefully(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            let pgid = -(pid as libc::pid_t);
            // SAFETY: libc::kill is safe to call with a valid negated pgid.
            unsafe { libc::kill(pgid, libc::SIGTERM) };
            tokio::time::sleep(Duration::from_secs(SIGTERM_GRACE_PERIOD_SECS)).await;
            // Force-kill anything still running after the grace period.
            unsafe { libc::kill(pgid, libc::SIGKILL) };
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

// ---------------------------------------------------------------------------
// Runtime protection indication
// ---------------------------------------------------------------------------

/// Print a startup banner to stderr showing the tool's protection status.
///
/// This provides immediate visual feedback that the broker is active and
/// which enforcement layers are protecting the tool.
fn print_startup_banner(config: &ProxyConfig) {
    let summary = config.sandbox.enforcement_summary();
    let profile = format!("{:?}", config.manifest.profile).to_lowercase();
    let layers = build_layer_description(&summary);

    eprintln!(
        "[mcparmor] {} protected | profile: {} | layers: {}",
        config.tool_name, profile, layers
    );
}

/// Build a human-readable description of the active enforcement layers.
fn build_layer_description(summary: &crate::sandbox::EnforcementSummary) -> String {
    let mut parts = vec!["protocol"];

    if summary.filesystem_isolation
        || summary.spawn_blocking
        || summary.network_port_enforcement
        || summary.network_hostname_enforcement
    {
        parts.push(&summary.mechanism);
    }

    parts.join("+")
}

/// Record the `id` of a `tools/list` request so the response can be annotated.
async fn track_tools_list_id(message: &Value, ids: &Arc<Mutex<HashSet<String>>>) {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    if method != "tools/list" {
        return;
    }
    if let Some(id) = message.get("id") {
        let id_str = serde_json::to_string(id).unwrap_or_default();
        ids.lock().await.insert(id_str);
    }
}

/// If the response matches a `tools/list` request, annotate each tool's description
/// with a shield indicator so the host UI shows protection status.
///
/// Returns the (possibly modified) line. Non-matching responses are returned unchanged.
async fn annotate_tools_list_response(
    line: &str,
    ids: &Arc<Mutex<HashSet<String>>>,
) -> String {
    let Ok(mut response) = serde_json::from_str::<Value>(line) else {
        return line.to_string();
    };

    // Check if this response's id matches a tracked tools/list request.
    let id = response.get("id");
    let id_str = id
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();

    let is_tools_list = ids.lock().await.remove(&id_str);
    if !is_tools_list {
        return line.to_string();
    }

    // Annotate each tool's description in the result.tools array.
    if let Some(tools) = response
        .get_mut("result")
        .and_then(|r| r.get_mut("tools"))
        .and_then(Value::as_array_mut)
    {
        for tool in tools {
            annotate_tool_description(tool);
        }

        if let Ok(annotated) = serde_json::to_string(&response) {
            return annotated;
        }
    }

    line.to_string()
}

/// Append the armor shield indicator to a single tool's description field.
fn annotate_tool_description(tool: &mut Value) {
    let desc = tool
        .get("description")
        .and_then(Value::as_str)
        .map(String::from);

    if let Some(desc) = desc {
        // Avoid double-annotation if the proxy is nested.
        if !desc.contains(ARMOR_ANNOTATION) {
            tool["description"] = Value::String(format!("{desc}{ARMOR_ANNOTATION}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcparmor_core::errors::codes;
    use serde_json::json;

    // ---------------------------------------------------------------------------
    // split_command
    // ---------------------------------------------------------------------------

    #[test]
    fn split_command_returns_program_and_empty_args_for_single_element() {
        let command = vec!["node".to_string()];
        let (prog, args) = split_command(&command).unwrap();
        assert_eq!(prog, "node");
        assert!(args.is_empty());
    }

    #[test]
    fn split_command_returns_program_and_args() {
        let command = vec![
            "python3".to_string(),
            "server.py".to_string(),
            "--port".to_string(),
            "3000".to_string(),
        ];
        let (prog, args) = split_command(&command).unwrap();
        assert_eq!(prog, "python3");
        assert_eq!(args, &["server.py", "--port", "3000"]);
    }

    #[test]
    fn split_command_returns_error_for_empty_slice() {
        let command: Vec<String> = vec![];
        assert!(split_command(&command).is_err(), "empty command must be an error");
    }

    // ---------------------------------------------------------------------------
    // truncate_utf8
    // ---------------------------------------------------------------------------

    #[test]
    fn truncate_utf8_returns_full_string_when_under_limit() {
        assert_eq!(truncate_utf8("hello", 10), "hello");
    }

    #[test]
    fn truncate_utf8_returns_full_string_at_exact_limit() {
        assert_eq!(truncate_utf8("hello", 5), "hello");
    }

    #[test]
    fn truncate_utf8_truncates_at_ascii_boundary() {
        assert_eq!(truncate_utf8("hello world", 5), "hello");
    }

    #[test]
    fn truncate_utf8_does_not_split_multibyte_character() {
        // "é" is 2 bytes in UTF-8 (0xC3 0xA9).
        // Truncating at 1 byte must back up to 0 bytes, not split the char.
        let s = "élan";
        let result = truncate_utf8(s, 1);
        assert!(result.is_empty(), "must back up past the multi-byte char, got: {result:?}");
    }

    #[test]
    fn truncate_utf8_handles_multibyte_at_exact_char_boundary() {
        // "é" (2 bytes) followed by ASCII. Truncate at byte 2 → keep "é".
        let s = "élan";
        let result = truncate_utf8(s, 2);
        assert_eq!(result, "é");
    }

    #[test]
    fn truncate_utf8_handles_empty_string() {
        assert_eq!(truncate_utf8("", 10), "");
        assert_eq!(truncate_utf8("", 0), "");
    }

    #[test]
    fn truncate_utf8_with_zero_limit_returns_empty() {
        assert_eq!(truncate_utf8("hello", 0), "");
    }

    #[test]
    fn truncate_utf8_unicode_emoji_not_split() {
        // "🔑" is 4 bytes. Truncating at 3 must back up to 0.
        let s = "🔑key";
        let result = truncate_utf8(s, 3);
        assert!(result.is_empty(), "3 bytes is mid-emoji; result must be empty, got: {result:?}");
    }

    // ---------------------------------------------------------------------------
    // apply_size_limit
    // ---------------------------------------------------------------------------

    #[test]
    fn apply_size_limit_returns_borrowed_when_no_limit() {
        let line = "hello world";
        let result = apply_size_limit(line, &None);
        assert_eq!(&*result, line);
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn apply_size_limit_returns_borrowed_when_under_limit() {
        let line = "hello";
        // 1 KB = 1024 bytes; "hello" is 5 bytes.
        let result = apply_size_limit(line, &Some(1));
        assert_eq!(&*result, line);
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn apply_size_limit_truncates_when_over_limit() {
        // 1 KB = 1024 bytes. Build a 2048-byte string.
        let line = "A".repeat(2048);
        let result = apply_size_limit(&line, &Some(1));
        assert_eq!(result.len(), 1024);
    }

    #[test]
    fn apply_size_limit_at_exact_boundary_is_not_truncated() {
        // Exactly 1024 bytes — must not be truncated.
        let line = "B".repeat(1024);
        let result = apply_size_limit(&line, &Some(1));
        assert_eq!(result.len(), 1024);
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn apply_size_limit_with_zero_kb_truncates_to_empty() {
        // 0 KB = 0 bytes — everything is truncated.
        let result = apply_size_limit("anything", &Some(0));
        assert_eq!(&*result, "");
    }

    // ---------------------------------------------------------------------------
    // build_error_response
    // ---------------------------------------------------------------------------

    #[test]
    fn build_error_response_preserves_request_id() {
        let request = json!({ "jsonrpc": "2.0", "id": 42, "method": "tools/call" });
        let err = mcparmor_core::errors::BrokerError::spawn_violation("curl");
        let response = build_error_response(&request, err);
        assert_eq!(response["id"], json!(42));
    }

    #[test]
    fn build_error_response_uses_null_id_when_absent() {
        let request = json!({ "jsonrpc": "2.0", "method": "tools/call" });
        let err = mcparmor_core::errors::BrokerError::spawn_violation("curl");
        let response = build_error_response(&request, err);
        assert_eq!(response["id"], json!(null));
    }

    #[test]
    fn build_error_response_includes_jsonrpc_field() {
        let request = json!({ "jsonrpc": "2.0", "id": 1 });
        let err = mcparmor_core::errors::BrokerError::timeout(5000);
        let response = build_error_response(&request, err);
        assert_eq!(response["jsonrpc"], "2.0");
    }

    #[test]
    fn build_error_response_includes_error_code_and_message() {
        let request = json!({ "id": 1 });
        let err = mcparmor_core::errors::BrokerError::path_violation("/etc/passwd");
        let response = build_error_response(&request, err);
        assert_eq!(response["error"]["code"], codes::PATH_VIOLATION);
        assert!(
            response["error"]["message"].as_str().unwrap().contains("/etc/passwd"),
            "message must embed the path"
        );
    }

    #[test]
    fn build_error_response_includes_hint_in_data() {
        let request = json!({ "id": 1 });
        let err = mcparmor_core::errors::BrokerError::network_violation("evil.com", 443);
        let response = build_error_response(&request, err);
        assert!(
            response["error"]["data"]["hint"].is_string(),
            "hint must be a string"
        );
    }

    #[test]
    fn build_error_response_with_string_id() {
        let request = json!({ "id": "req-123" });
        let err = mcparmor_core::errors::BrokerError::manifest_error("missing version");
        let response = build_error_response(&request, err);
        assert_eq!(response["id"], "req-123");
    }

    // ---------------------------------------------------------------------------
    // extract_id_from_line
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_id_from_line_returns_integer_id() {
        let line = r#"{"jsonrpc":"2.0","id":99,"method":"tools/call"}"#;
        assert_eq!(extract_id_from_line(line), json!(99));
    }

    #[test]
    fn extract_id_from_line_returns_string_id() {
        let line = r#"{"jsonrpc":"2.0","id":"abc"}"#;
        assert_eq!(extract_id_from_line(line), json!("abc"));
    }

    #[test]
    fn extract_id_from_line_returns_null_when_id_absent() {
        let line = r#"{"jsonrpc":"2.0","method":"initialize"}"#;
        assert_eq!(extract_id_from_line(line), json!(null));
    }

    #[test]
    fn extract_id_from_line_returns_null_for_malformed_json() {
        assert_eq!(extract_id_from_line("not json at all"), json!(null));
    }

    #[test]
    fn extract_id_from_line_returns_null_for_empty_string() {
        assert_eq!(extract_id_from_line(""), json!(null));
    }

    // ---------------------------------------------------------------------------
    // build_invoke_detail
    // ---------------------------------------------------------------------------

    #[test]
    fn build_invoke_detail_without_arguments_returns_tools_call_string() {
        let message = json!({ "jsonrpc": "2.0", "method": "tools/call", "id": 1 });
        let detail = build_invoke_detail(&message, false);
        assert_eq!(detail, "tools/call");
    }

    #[test]
    fn build_invoke_detail_with_arguments_returns_json_object() {
        let message = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "arguments": { "path": "/tmp/file", "mode": "read" } }
        });
        let detail = build_invoke_detail(&message, false);
        // Must contain the argument values when no_log_params is false.
        assert!(detail.contains("/tmp/file"), "detail must include arg values: {detail}");
        assert!(detail.contains("mode"), "detail must include arg keys: {detail}");
    }

    #[test]
    fn build_invoke_detail_with_no_log_params_omits_values() {
        let message = json!({
            "jsonrpc": "2.0", "method": "tools/call", "id": 1,
            "params": { "arguments": { "secret_key": "supersecret123", "tool": "bash" } }
        });
        let detail = build_invoke_detail(&message, true);
        // Values must not appear; keys should be present.
        assert!(!detail.contains("supersecret123"), "values must be omitted: {detail}");
        assert!(detail.contains("secret_key"), "keys must still be present: {detail}");
        assert!(detail.contains("tool"), "keys must still be present: {detail}");
    }

    #[test]
    fn build_invoke_detail_no_log_params_with_no_arguments_returns_tools_call() {
        let message = json!({ "method": "tools/call" });
        let detail = build_invoke_detail(&message, true);
        assert_eq!(detail, "tools/call");
    }

    #[test]
    fn build_invoke_detail_empty_arguments_object_with_no_log_params() {
        let message = json!({
            "params": { "arguments": {} }
        });
        let detail = build_invoke_detail(&message, true);
        // Empty keys list.
        assert!(detail.contains("keys="), "must show keys= prefix: {detail}");
    }

    // ---------------------------------------------------------------------------
    // GAP 1: ProxyConfig has strict_mode and verbose fields
    // ---------------------------------------------------------------------------

    #[test]
    fn proxy_config_strict_mode_field_is_false_by_default_construction() {
        // Verify the struct can be built with strict_mode=false and verbose=false.
        // This confirms the fields exist and the struct is valid.
        use crate::sandbox::noop::NoopSandbox;
        use crate::audit_writer::AuditWriter;
        use mcparmor_core::manifest::{
            ArmorManifest, Profile, FilesystemPolicy, NetworkPolicy, EnvPolicy, OutputPolicy, AuditPolicy,
        };
        use std::sync::Arc;

        let manifest = ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Strict,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy::default(),
            spawn: false,
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        };

        let config = ProxyConfig {
            manifest: Arc::new(manifest),
            sandbox: Arc::new(NoopSandbox),
            audit_writer: Arc::new(AuditWriter::new(
                std::path::PathBuf::from("/tmp/test_audit.log"),
                None,
                None,
            )),
            no_os_sandbox: false,
            no_log_params: false,
            strict_mode: false,
            verbose: false,
            tool_name: "test-tool".to_string(),
            annotate: true,
        };

        assert!(!config.strict_mode, "strict_mode must default to false");
        assert!(!config.verbose, "verbose must default to false");
        assert!(config.annotate, "annotate must default to true");
    }

    #[test]
    fn proxy_config_strict_mode_field_can_be_set_true() {
        use crate::sandbox::noop::NoopSandbox;
        use crate::audit_writer::AuditWriter;
        use mcparmor_core::manifest::{
            ArmorManifest, Profile, FilesystemPolicy, NetworkPolicy, EnvPolicy, OutputPolicy, AuditPolicy,
        };
        use std::sync::Arc;

        let manifest = ArmorManifest {
            version: "1.0".to_string(),
            profile: Profile::Strict,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy::default(),
            spawn: false,
            env: EnvPolicy::default(),
            output: OutputPolicy::default(),
            audit: AuditPolicy::default(),
            timeout_ms: None,
            locked: false,
            min_spec: None,
        };

        let config = ProxyConfig {
            manifest: Arc::new(manifest),
            sandbox: Arc::new(NoopSandbox),
            audit_writer: Arc::new(AuditWriter::new(
                std::path::PathBuf::from("/tmp/test_audit_strict.log"),
                None,
                None,
            )),
            no_os_sandbox: false,
            no_log_params: false,
            strict_mode: true,
            verbose: true,
            tool_name: "test-tool".to_string(),
            annotate: true,
        };

        assert!(config.strict_mode, "strict_mode must be true when set");
        assert!(config.verbose, "verbose must be true when set");
    }

    // ---------------------------------------------------------------------------
    // annotate_tool_description
    // ---------------------------------------------------------------------------

    #[test]
    fn annotate_tool_description_appends_shield_to_description() {
        let mut tool = json!({
            "name": "read_file",
            "description": "Read the contents of a file"
        });
        annotate_tool_description(&mut tool);
        assert_eq!(
            tool["description"],
            "Read the contents of a file [🛡 MCP Armor]"
        );
    }

    #[test]
    fn annotate_tool_description_does_not_double_annotate() {
        let mut tool = json!({
            "name": "read_file",
            "description": "Read a file [🛡 MCP Armor]"
        });
        annotate_tool_description(&mut tool);
        assert_eq!(
            tool["description"],
            "Read a file [🛡 MCP Armor]",
            "must not double-annotate"
        );
    }

    #[test]
    fn annotate_tool_description_skips_tool_without_description() {
        let mut tool = json!({ "name": "ping" });
        annotate_tool_description(&mut tool);
        assert!(tool.get("description").is_none(), "must not add description if absent");
    }

    // ---------------------------------------------------------------------------
    // annotate_tools_list_response
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn annotate_tools_list_response_annotates_matching_response() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        ids.lock().await.insert("1".to_string());

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "tools": [
                    { "name": "read_file", "description": "Read a file" },
                    { "name": "write_file", "description": "Write a file" }
                ]
            }
        });
        let line = serde_json::to_string(&response).unwrap();

        let result = annotate_tools_list_response(&line, &ids).await;
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert!(
            parsed["result"]["tools"][0]["description"]
                .as_str()
                .unwrap()
                .contains(ARMOR_ANNOTATION),
            "first tool must be annotated"
        );
        assert!(
            parsed["result"]["tools"][1]["description"]
                .as_str()
                .unwrap()
                .contains(ARMOR_ANNOTATION),
            "second tool must be annotated"
        );
    }

    #[tokio::test]
    async fn annotate_tools_list_response_ignores_non_matching_id() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        ids.lock().await.insert("99".to_string());

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "tools": [{ "name": "read_file", "description": "Read a file" }]
            }
        });
        let line = serde_json::to_string(&response).unwrap();

        let result = annotate_tools_list_response(&line, &ids).await;
        assert_eq!(result, line, "non-matching response must be unchanged");
    }

    #[tokio::test]
    async fn annotate_tools_list_response_handles_malformed_json() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let result = annotate_tools_list_response("not json", &ids).await;
        assert_eq!(result, "not json", "malformed JSON must be returned as-is");
    }

    // ---------------------------------------------------------------------------
    // track_tools_list_id
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn track_tools_list_id_records_id_for_tools_list_method() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let message = json!({ "jsonrpc": "2.0", "method": "tools/list", "id": 42 });
        track_tools_list_id(&message, &ids).await;
        assert!(ids.lock().await.contains("42"), "id must be tracked");
    }

    #[tokio::test]
    async fn track_tools_list_id_ignores_non_tools_list_method() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let message = json!({ "jsonrpc": "2.0", "method": "tools/call", "id": 42 });
        track_tools_list_id(&message, &ids).await;
        assert!(ids.lock().await.is_empty(), "non-tools/list must not be tracked");
    }

    #[tokio::test]
    async fn track_tools_list_id_handles_string_id() {
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let message = json!({ "jsonrpc": "2.0", "method": "tools/list", "id": "req-1" });
        track_tools_list_id(&message, &ids).await;
        assert!(ids.lock().await.contains("\"req-1\""), "string id must be tracked");
    }

    // ---------------------------------------------------------------------------
    // build_layer_description
    // ---------------------------------------------------------------------------

    #[test]
    fn build_layer_description_protocol_only_when_no_os_enforcement() {
        let summary = crate::sandbox::EnforcementSummary {
            filesystem_isolation: false,
            spawn_blocking: false,
            network_port_enforcement: false,
            network_hostname_enforcement: false,
            mechanism: "none".to_string(),
        };
        assert_eq!(build_layer_description(&summary), "protocol");
    }

    #[test]
    fn build_layer_description_includes_mechanism_when_os_enforced() {
        let summary = crate::sandbox::EnforcementSummary {
            filesystem_isolation: true,
            spawn_blocking: true,
            network_port_enforcement: false,
            network_hostname_enforcement: true,
            mechanism: "seatbelt".to_string(),
        };
        assert_eq!(build_layer_description(&summary), "protocol+seatbelt");
    }
}

