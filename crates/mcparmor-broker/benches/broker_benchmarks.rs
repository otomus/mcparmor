//! Criterion benchmarks for mcparmor-broker hot paths.
//!
//! Performance budgets (P99 targets from the execution plan):
//! - Param inspection (10 params): < 1ms
//! - SBPL profile generation:      < 2ms (macOS only)

use criterion::{criterion_group, criterion_main, Criterion};
use mcparmor_broker::inspect;
use mcparmor_core::manifest::{
    ArmorManifest, FilesystemPolicy, NetworkPolicy, Profile,
};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

fn sandboxed_manifest() -> ArmorManifest {
    ArmorManifest {
        version: "1.0".to_string(),
        min_spec: None,
        profile: Profile::Sandboxed,
        locked: false,
        timeout_ms: None,
        filesystem: FilesystemPolicy {
            read: vec!["/tmp/mcparmor/*".to_string()],
            write: vec!["/tmp/mcparmor/*".to_string()],
        },
        network: NetworkPolicy {
            allow: vec!["api.github.com:443".to_string()],
            deny_local: true,
            deny_metadata: true,
        },
        spawn: false,
        env: Default::default(),
        output: Default::default(),
        audit: Default::default(),
    }
}

fn tools_call_message(param_count: usize) -> Value {
    let mut params = serde_json::Map::new();
    for i in 0..param_count {
        params.insert(format!("key{i}"), json!(format!("/tmp/mcparmor/file{i}.txt")));
    }
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "my_tool",
            "arguments": params
        }
    })
}

// ---------------------------------------------------------------------------
// Benchmark 1: Layer 1 param inspection
// ---------------------------------------------------------------------------

/// Benchmarks `inspect::check_message` with a realistic 10-parameter tools/call.
///
/// Verifies the < 1ms budget for protocol-level enforcement.
fn bench_param_inspect(c: &mut Criterion) {
    let manifest = sandboxed_manifest();
    let message = tools_call_message(10);

    let mut group = c.benchmark_group("param_inspect");
    group.bench_function("tools_call_10_params", |b| {
        b.iter(|| inspect::check_message(&message, &manifest));
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark 2: SBPL profile generation (macOS only)
// ---------------------------------------------------------------------------

/// Benchmarks Seatbelt SBPL profile generation from a sandboxed manifest.
///
/// Only compiled and run on macOS. Verifies the < 2ms budget for sandbox
/// profile construction at spawn time.
#[cfg(target_os = "macos")]
fn bench_seatbelt_generate(c: &mut Criterion) {
    use mcparmor_broker::sandbox::macos::generate_sbpl_profile;

    let manifest = sandboxed_manifest();
    let tool_path = "/usr/local/bin/my-mcp-tool";

    let mut group = c.benchmark_group("seatbelt_generate");
    group.bench_function("sandboxed_manifest", |b| {
        b.iter(|| generate_sbpl_profile(&manifest, tool_path).expect("SBPL generation failed"));
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion entry points
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
criterion_group!(benches, bench_param_inspect, bench_seatbelt_generate);

#[cfg(not(target_os = "macos"))]
criterion_group!(benches, bench_param_inspect);

criterion_main!(benches);
