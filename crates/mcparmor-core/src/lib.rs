//! MCP Armor core library.
//!
//! Provides manifest parsing, enforcement types, secret scanning, and
//! audit logging primitives used by the broker and all language SDKs.

pub mod audit;
pub mod errors;
pub mod manifest;
pub mod policy;
pub mod scanner;
