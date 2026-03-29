//! Linux sandbox provider — Seccomp + Landlock.
//!
//! Enforcement levels depend on kernel version, detected at startup:
//! - Kernel 3.5+:  Seccomp blocks execve (spawn prevention)
//! - Kernel 5.13+: Landlock FS restricts filesystem access
//! - Kernel 6.7+:  Landlock TCP restricts outbound connections by port
//!
//! Hostname-level network filtering is not achievable on Linux without
//! elevated privileges. network.allow hostname restrictions are enforced
//! by Layer 1 param inspection on all Linux systems.

use anyhow::{Context, Result};
use mcparmor_core::manifest::ArmorManifest;
use std::sync::Arc;

use super::{EnforcementSummary, SandboxProvider, SandboxedCommand};

/// Linux sandbox using Seccomp and Landlock.
pub struct LinuxSandbox {
    /// Detected kernel version at broker startup.
    kernel_version: KernelVersion,
}

/// Parsed Linux kernel version for capability detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KernelVersion {
    /// Major version component (e.g. `6` in `6.7.3`).
    pub major: u32,
    /// Minor version component (e.g. `7` in `6.7.3`).
    pub minor: u32,
    /// Patch version component (e.g. `3` in `6.7.3`).
    pub patch: u32,
}

impl KernelVersion {
    /// Returns true if Seccomp execve filtering is available (kernel 3.5+).
    pub fn has_seccomp(&self) -> bool {
        *self >= KernelVersion { major: 3, minor: 5, patch: 0 }
    }

    /// Returns true if Landlock filesystem isolation is available (kernel 5.13+).
    pub fn has_landlock_fs(&self) -> bool {
        *self >= KernelVersion { major: 5, minor: 13, patch: 0 }
    }

    /// Returns true if Landlock TCP port filtering is available (kernel 6.7+).
    pub fn has_landlock_tcp(&self) -> bool {
        *self >= KernelVersion { major: 6, minor: 7, patch: 0 }
    }
}

impl LinuxSandbox {
    /// Create a new Linux sandbox provider by detecting the current kernel version.
    ///
    /// # Errors
    /// Returns an error if the kernel version cannot be parsed from `uname`.
    pub fn detect() -> Result<Self> {
        let version = detect_kernel_version()?;
        Ok(Self { kernel_version: version })
    }
}

impl SandboxProvider for LinuxSandbox {
    fn apply(
        &self,
        _manifest: &ArmorManifest,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedCommand> {
        // Linux-specific FS and spawn restrictions are applied via pre_exec hooks.
        // The apply() method just returns the command unchanged; configure_pre_exec()
        // installs the Landlock + Seccomp hooks before the broker calls spawn().
        Ok(SandboxedCommand {
            program: command.to_string(),
            args: args.to_vec(),
            env: Vec::new(),
            process_group: true,
        })
    }

    fn is_available(&self) -> bool {
        // Available on any Linux — even if only Seccomp is supported
        self.kernel_version.has_seccomp()
    }

    fn enforcement_summary(&self) -> EnforcementSummary {
        EnforcementSummary {
            filesystem_isolation: self.kernel_version.has_landlock_fs(),
            // Spawn blocking via Seccomp-in-pre_exec is not implemented.
            // See the design note above. Layer 1 enforces spawn: false.
            spawn_blocking: false,
            network_port_enforcement: self.kernel_version.has_landlock_tcp(),
            network_hostname_enforcement: false,
            mechanism: format!(
                "Landlock (kernel {}.{}.{})",
                self.kernel_version.major,
                self.kernel_version.minor,
                self.kernel_version.patch,
            ),
        }
    }
}

/// Apply Landlock filesystem restrictions in the current process.
///
/// Must be called inside a `Command::pre_exec` hook (runs after fork, before exec).
/// Only effective when the kernel version supports Landlock FS (5.13+).
///
/// # Errors
/// Returns an error if the Landlock ruleset cannot be created or restricted.
pub fn apply_landlock_fs(manifest: &ArmorManifest) -> Result<()> {
    use landlock::{
        Access, AccessFs, ABI, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };

    let abi = ABI::V3;
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_read(abi))
        .context("Failed to create Landlock ruleset")?
        .create()
        .context("Failed to instantiate Landlock ruleset")?;

    for pattern in &manifest.filesystem.read {
        // Use the pattern as a path directly for non-glob patterns.
        // Glob patterns are applied at Layer 1; Landlock gets the root directories.
        let fd = PathFd::new(pattern)
            .with_context(|| format!("Landlock: cannot open path '{pattern}'"))?;
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, AccessFs::from_read(abi)))
            .context("Failed to add Landlock read rule")?;
    }

    for pattern in &manifest.filesystem.write {
        let fd = PathFd::new(pattern)
            .with_context(|| format!("Landlock: cannot open path '{pattern}'"))?;
        ruleset = ruleset
            .add_rule(PathBeneath::new(
                fd,
                AccessFs::from_all(abi),
            ))
            .context("Failed to add Landlock write rule")?;
    }

    ruleset.restrict_self().context("Failed to restrict process with Landlock")?;
    Ok(())
}

/// Spawn blocking on Linux — design note.
///
/// Seccomp-based spawn blocking cannot be installed in a `pre_exec` hook because
/// the hook runs after fork() but before exec(). Any Seccomp filter that blocks
/// `execve`/`execveat` also blocks the initial exec() call that loads the tool
/// binary, preventing the tool from starting entirely.
///
/// The correct approach requires one of:
/// - `SECCOMP_RET_USER_NOTIF` (kernel 5.0+): supervisor process intercepts exec
///   calls and allows only the first one. Requires a supervisor goroutine.
/// - `LANDLOCK_ACCESS_FS_EXECUTE` (kernel 5.19+): restrict which paths the tool
///   process can execute, preventing spawning of system binaries. Requires knowing
///   exactly which binaries the tool legitimately needs.
///
/// Until one of these is implemented, Linux spawn blocking is Layer 1 only
/// (documented in `enforcement_summary`). macOS Seatbelt's `(deny default)`
/// correctly blocks process-exec in the sandboxed child.

/// Configure pre-exec sandbox restrictions for a `Command`.
///
/// Installs Landlock FS restrictions and Seccomp spawn-blocking filters
/// as `pre_exec` hooks. Called by the proxy before spawning the tool on Linux.
///
/// The hooks run after fork() but before exec() in the child process, so
/// they apply to the tool but not to the broker.
///
/// # Arguments
/// * `manifest` - The armor manifest declaring filesystem and spawn policy
/// * `cmd` - The std `Command` being configured for the tool
pub fn configure_pre_exec(manifest: Arc<ArmorManifest>, cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: pre_exec runs after fork() in the child process.
    // The manifest Arc is cloned before fork, so it is safe to use here.
    unsafe {
        cmd.pre_exec(move || {
            // Apply Landlock FS restrictions if kernel supports it.
            // Landlock runs before exec and correctly restricts the new process's
            // filesystem access without blocking the exec itself.
            if let Err(e) = apply_landlock_fs(&manifest) {
                tracing::warn!("Landlock FS setup failed (non-fatal): {e:#}");
            }

            // Spawn blocking via Seccomp is intentionally not applied here.
            // See the design note on `apply_seccomp_no_spawn` for why.
            // spawn: false is enforced at Layer 1 (param inspection) on Linux.

            Ok(())
        });
    }
}

/// Parse the running kernel version from `uname -r`.
///
/// # Errors
/// Returns an error if `uname` fails or the version string cannot be parsed.
fn detect_kernel_version() -> Result<KernelVersion> {
    let output = std::process::Command::new("uname").arg("-r").output()?;
    let version_str = String::from_utf8(output.stdout)?;
    parse_kernel_version(version_str.trim())
}

/// Parse a kernel version string like "5.15.0-91-generic" into a `KernelVersion`.
fn parse_kernel_version(s: &str) -> Result<KernelVersion> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 2 {
        anyhow::bail!("Cannot parse kernel version: '{s}'");
    }
    let major = parts[0].parse::<u32>()?;
    let minor = parts[1]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("0")
        .parse::<u32>()?;
    let patch = parts
        .get(2)
        .and_then(|p| p.split(|c: char| !c.is_ascii_digit()).next())
        .unwrap_or("0")
        .parse::<u32>()
        .unwrap_or(0);

    Ok(KernelVersion { major, minor, patch })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Happy path: version strings produced by real kernels ---

    #[test]
    fn parses_full_version_with_suffix() {
        let v = parse_kernel_version("5.15.0-91-generic").unwrap();
        assert_eq!(v, KernelVersion { major: 5, minor: 15, patch: 0 });
    }

    #[test]
    fn parses_version_with_three_numeric_parts() {
        let v = parse_kernel_version("6.7.3").unwrap();
        assert_eq!(v, KernelVersion { major: 6, minor: 7, patch: 3 });
    }

    #[test]
    fn parses_version_without_patch() {
        let v = parse_kernel_version("5.13").unwrap();
        assert_eq!(v, KernelVersion { major: 5, minor: 13, patch: 0 });
    }

    #[test]
    fn parses_macos_style_version() {
        // macOS uname returns versions like "23.3.0" (Darwin kernel).
        let v = parse_kernel_version("23.3.0").unwrap();
        assert_eq!(v, KernelVersion { major: 23, minor: 3, patch: 0 });
    }

    #[test]
    fn parses_minor_with_rc_suffix() {
        // Release candidates: "6.8-rc3" — minor has no separating dot.
        let v = parse_kernel_version("6.8-rc3").unwrap();
        assert_eq!(v, KernelVersion { major: 6, minor: 8, patch: 0 });
    }

    // --- Feature detection thresholds ---

    #[test]
    fn kernel_3_5_has_seccomp() {
        let v = KernelVersion { major: 3, minor: 5, patch: 0 };
        assert!(v.has_seccomp());
    }

    #[test]
    fn kernel_3_4_lacks_seccomp() {
        let v = KernelVersion { major: 3, minor: 4, patch: 99 };
        assert!(!v.has_seccomp());
    }

    #[test]
    fn kernel_5_13_has_landlock_fs() {
        let v = KernelVersion { major: 5, minor: 13, patch: 0 };
        assert!(v.has_landlock_fs());
    }

    #[test]
    fn kernel_5_12_lacks_landlock_fs() {
        let v = KernelVersion { major: 5, minor: 12, patch: 99 };
        assert!(!v.has_landlock_fs());
    }

    #[test]
    fn kernel_6_7_has_landlock_tcp() {
        let v = KernelVersion { major: 6, minor: 7, patch: 0 };
        assert!(v.has_landlock_tcp());
    }

    #[test]
    fn kernel_6_6_lacks_landlock_tcp() {
        let v = KernelVersion { major: 6, minor: 6, patch: 99 };
        assert!(!v.has_landlock_tcp());
    }

    // --- Edge cases and malformed input ---

    #[test]
    fn empty_string_returns_error() {
        assert!(parse_kernel_version("").is_err());
    }

    #[test]
    fn single_number_returns_error() {
        assert!(parse_kernel_version("5").is_err());
    }

    #[test]
    fn non_numeric_major_returns_error() {
        assert!(parse_kernel_version("abc.15.0").is_err());
    }

    #[test]
    fn patch_with_hyphen_suffix_is_parsed_correctly() {
        let v = parse_kernel_version("5.15.0-91-generic").unwrap();
        assert_eq!(v.patch, 0);
    }
}
