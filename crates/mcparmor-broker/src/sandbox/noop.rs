//! No-op sandbox provider — Layer 1 only.
//!
//! Used when no OS sandbox primitives are available (Windows v1, very old Linux kernels).
//! Layer 1 protocol enforcement (param inspection, secret scanning, audit) remains active.
//! Layer 2 kernel enforcement is absent — this is documented honestly in `mcparmor status`.

use anyhow::Result;
use mcparmor_core::manifest::ArmorManifest;

use super::{EnforcementSummary, SandboxProvider, SandboxedCommand};

/// A sandbox provider that performs no OS-level isolation.
///
/// All Layer 1 enforcement (param inspection, secret scanning, env stripping,
/// audit logging) remains fully active. Only kernel-level syscall restriction
/// is absent.
pub struct NoopSandbox;

impl SandboxProvider for NoopSandbox {
    fn apply(
        &self,
        _manifest: &ArmorManifest,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedCommand> {
        Ok(SandboxedCommand {
            program: command.to_string(),
            args: args.to_vec(),
            env: Vec::new(),
            process_group: false,
        })
    }

    fn is_available(&self) -> bool {
        true
    }

    fn enforcement_summary(&self) -> EnforcementSummary {
        EnforcementSummary {
            filesystem_isolation: false,
            spawn_blocking: false,
            network_port_enforcement: false,
            network_hostname_enforcement: false,
            mechanism: "none — protocol-layer enforcement only".to_string(),
        }
    }
}
