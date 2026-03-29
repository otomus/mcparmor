//! MCP Armor broker — entry point.
//!
//! Parses CLI arguments and dispatches to the appropriate subcommand.
//! The broker itself (the `run` subcommand) acts as a stdio proxy between
//! the MCP host and the tool subprocess, enforcing the declared armor manifest.

use anyhow::Result;
use clap::Parser;
use mcparmor_broker::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    cli.execute().await
}
