//! CLI argument definitions for all mcparmor subcommands.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// MCP Armor — capability enforcement for MCP tools.
#[derive(Parser, Debug)]
#[command(name = "mcparmor", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run an MCP tool under armor enforcement.
    Run(RunArgs),
    /// Wrap a host config to route all stdio tools through the broker.
    Wrap(WrapArgs),
    /// Restore a host config to its pre-wrap state.
    Unwrap(UnwrapArgs),
    /// Show the current protection state for every tool in a host config.
    Status(StatusArgs),
    /// Validate an armor.json manifest against the spec schema.
    Validate(ValidateArgs),
    /// Query the armor audit log.
    Audit(AuditArgs),
    /// Generate a minimal armor.json interactively.
    Init(InitArgs),
    /// Manage armor profiles (list, show, update, add).
    Profiles(ProfilesArgs),
}

impl Cli {
    /// Dispatch to the appropriate subcommand handler.
    ///
    /// # Errors
    /// Propagates any error returned by the subcommand handler.
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Command::Run(args) => crate::broker::run(args).await,
            Command::Wrap(args) => crate::broker::wrap(args).await,
            Command::Unwrap(args) => crate::broker::unwrap(args).await,
            Command::Status(args) => crate::broker::status(args).await,
            Command::Validate(args) => crate::broker::validate(args).await,
            Command::Audit(args) => crate::broker::audit(args).await,
            Command::Init(args) => crate::broker::init(args).await,
            Command::Profiles(args) => crate::broker::profiles(args).await,
        }
    }
}

/// Arguments for `mcparmor run`.
#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Path to the armor.json manifest. Defaults to armor.json in the tool directory.
    #[arg(long, short = 'a')]
    pub armor: Option<PathBuf>,

    /// Override the base profile declared in armor.json.
    /// Ignored if the manifest sets locked: true.
    #[arg(long)]
    pub profile: Option<String>,

    /// Disable OS sandbox (Layer 2). Layer 1 protocol enforcement remains active.
    #[arg(long)]
    pub no_os_sandbox: bool,

    /// Omit parameter values from audit log entries (log keys only).
    #[arg(long)]
    pub no_log_params: bool,

    /// Treat any capability violation as fatal: exit with code 2 immediately.
    /// In non-strict mode (default), violations are blocked and logged but the
    /// tool session continues.
    #[arg(long)]
    pub strict: bool,

    /// Print capability decisions to stderr for every JSON-RPC message.
    /// Shows allow/deny for each param inspection and secret scan result.
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// Write audit log entries to this file instead of the default path.
    #[arg(long)]
    pub audit_log: Option<PathBuf>,

    /// Disable audit logging for this invocation. No audit entries are written.
    #[arg(long)]
    pub no_audit: bool,

    /// The tool command and arguments, separated from broker args by `--`.
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

/// Arguments for `mcparmor wrap`.
#[derive(Parser, Debug)]
pub struct WrapArgs {
    /// Target MCP host to wrap (e.g. claude-desktop, cursor, vscode-project).
    /// Optional when --config is supplied directly.
    #[arg(long)]
    pub host: Option<String>,

    /// Override the host config file path. Takes precedence over --host.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Scope for hosts with multiple config files (project, global, both).
    #[arg(long, default_value = "both")]
    pub scope: String,

    /// Re-wrap already-wrapped entries (updates armor path/profile).
    #[arg(long)]
    pub rewrap: bool,

    /// Show what would change without modifying any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Create a .bak copy of the original config before modifying it.
    #[arg(long, default_value_t = true)]
    pub backup: bool,

    /// Embed this profile override in each wrapped entry's run args.
    /// When set, each tool is wrapped as: mcparmor run --profile <PROFILE> -- <cmd>.
    #[arg(long)]
    pub profile: Option<String>,

    /// Do not include `--armor <path>` in the wrapped args, even when a matching
    /// armor.json is discovered. The broker will resolve the profile at startup via
    /// upward directory search and community profile fallback.
    ///
    /// Use this when committing wrapped configs to version control so the config
    /// works across machines without machine-specific absolute paths.
    #[arg(long)]
    pub no_armor_path: bool,
}

/// Arguments for `mcparmor unwrap`.
#[derive(Parser, Debug)]
pub struct UnwrapArgs {
    /// Target MCP host to unwrap (e.g. claude-desktop, cursor, vscode-project).
    /// Optional when --config is supplied directly.
    #[arg(long)]
    pub host: Option<String>,

    /// Override the host config file path. Takes precedence over --host.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// Arguments for `mcparmor status`.
#[derive(Parser, Debug)]
pub struct StatusArgs {
    /// Target host to show status for. Defaults to all detected hosts.
    #[arg(long)]
    pub host: Option<String>,

    /// Output format.
    #[arg(long, default_value = "table")]
    pub format: String,
}

/// Arguments for `mcparmor validate`.
#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Path to the armor.json to validate. Defaults to ./armor.json.
    #[arg(long, short = 'a')]
    pub armor: Option<PathBuf>,
}

/// Arguments for `mcparmor audit`.
#[derive(Parser, Debug)]
pub struct AuditArgs {
    /// Filter by tool name.
    #[arg(long)]
    pub tool: Option<String>,

    /// Filter by event type (invoke, violation, secret_detected, response).
    #[arg(long)]
    pub event: Option<String>,

    /// Filter by time (ISO8601 or relative: 1h, 24h, 7d).
    #[arg(long)]
    pub since: Option<String>,

    /// Output format.
    #[arg(long, default_value = "table")]
    pub format: String,

    /// Remove entries older than the configured retention period.
    #[arg(long)]
    pub prune: bool,

    /// Print audit log statistics.
    #[arg(long)]
    pub stats: bool,
}

/// Arguments for `mcparmor init`.
#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Directory to write armor.json to. Defaults to current directory.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Start with this base profile.
    #[arg(long)]
    pub profile: Option<String>,

    /// Overwrite an existing armor.json without prompting.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for `mcparmor profiles`.
#[derive(Parser, Debug)]
pub struct ProfilesArgs {
    #[command(subcommand)]
    pub command: ProfilesCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProfilesCommand {
    /// List all available profiles.
    List,
    /// Show the full armor.json for a named profile.
    Show { name: String },
    /// Fetch the latest community profiles from GitHub (SHA256 verified).
    Update,
    /// Install a local armor.json as a named user profile.
    Add { file: PathBuf },
}
