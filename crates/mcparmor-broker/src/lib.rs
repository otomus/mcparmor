//! MCP Armor broker — library crate.
//!
//! Exposes internal modules for benchmarks and integration tests.
//! The `mcparmor` binary entry point is in `main.rs` and delegates here.

pub mod audit_writer;
pub mod broker;
pub mod cli;
pub mod inspect;
pub mod proxy;
pub mod sandbox;
