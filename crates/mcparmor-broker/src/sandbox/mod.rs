//! OS sandbox providers — Layer 2 enforcement.
//!
//! The `SandboxProvider` trait abstracts over platform-specific sandboxing
//! mechanisms. The broker selects the appropriate provider at startup based
//! on the detected platform and kernel version.
//!
//! Implemented providers:
//! - `LinuxSandbox`: Seccomp (spawn blocking, kernel 3.5+) + Landlock (FS/network, kernel 5.13+/6.7+)
//! - `MacosSeatbelt`: sandbox-exec (FS + network + spawn, macOS 12+)
//! - `NoopSandbox`: Layer 1 only — used on Windows or when kernel primitives are unavailable
//!
//! The `MacosContainer` provider (Apple Container framework, macOS 26+) is a v2 stub.
//! The `SandboxProvider` trait is designed to accommodate this swap without broker changes.

use anyhow::Result;
use mcparmor_core::manifest::ArmorManifest;

pub mod noop;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

/// Summary of what enforcement is active on the current platform.
#[derive(Debug, Clone)]
pub struct EnforcementSummary {
    /// Whether filesystem isolation is enforced at the OS level.
    pub filesystem_isolation: bool,
    /// Whether spawn blocking is enforced at the OS level.
    pub spawn_blocking: bool,
    /// Whether network blocking by port is enforced at the OS level.
    pub network_port_enforcement: bool,
    /// Whether hostname-level network blocking is enforced at the OS level.
    pub network_hostname_enforcement: bool,
    /// Human-readable description of the sandbox mechanism in use.
    pub mechanism: String,
}

/// A command that has been configured for sandboxed execution.
// Some fields are only read in platform-specific code paths.
#[allow(dead_code)]
pub struct SandboxedCommand {
    /// The program to execute.
    pub program: String,
    /// Arguments to pass to the program.
    pub args: Vec<String>,
    /// Environment variables to pass to the process.
    pub env: Vec<(String, String)>,
    /// Process group ID assigned at spawn (for kill(-pgid) on timeout).
    pub process_group: bool,
}

/// Trait for OS sandbox providers.
///
/// Each provider wraps the tool command in the appropriate OS primitive.
/// The broker calls `apply()` at spawn time, then executes the returned
/// `SandboxedCommand` in place of the original tool command.
///
/// The trait is `Send + Sync` so providers can be stored in `Arc<dyn SandboxProvider>`.
pub trait SandboxProvider: Send + Sync {
    /// Apply the sandbox to a tool command.
    ///
    /// Returns a `SandboxedCommand` that wraps the original command in the
    /// appropriate OS primitive (sandbox-exec, Landlock restrictions, etc.)
    ///
    /// # Arguments
    /// * `manifest` - The parsed armor manifest for the tool
    /// * `command` - The original tool command
    /// * `args` - The original tool arguments
    ///
    /// # Errors
    /// Returns an error if the sandbox cannot be applied (e.g. SBPL generation failure).
    fn apply(
        &self,
        manifest: &ArmorManifest,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedCommand>;

    /// Returns true if this sandbox provider is available on the current system.
    fn is_available(&self) -> bool;

    /// Returns a human-readable summary of what this provider enforces.
    fn enforcement_summary(&self) -> EnforcementSummary;
}
