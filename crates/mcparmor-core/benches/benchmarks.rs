//! Criterion benchmarks for mcparmor-core hot paths.
//!
//! Performance budgets (P99 targets from the execution plan):
//! - Manifest parse:       < 1ms
//! - Secret scan 10KB:     < 3ms
//! - Secret scan 100KB:    < 15ms

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mcparmor_core::manifest::ArmorManifest;
use mcparmor_core::scanner;

// ---------------------------------------------------------------------------
// Benchmark 1: Manifest parsing
// ---------------------------------------------------------------------------

/// Benchmarks `serde_json` deserialization of representative manifest shapes.
///
/// Covers the three payload sizes most commonly seen in the wild:
/// minimal (two fields), sandboxed (full policy block), and full (all fields).
fn bench_manifest_parse(c: &mut Criterion) {
    let manifests = [
        ("minimal", r#"{"version":"1.0","profile":"strict"}"#),
        (
            "sandboxed",
            r#"{
                "version": "1.0",
                "profile": "sandboxed",
                "filesystem": {
                    "read":  ["/tmp/mcparmor/*"],
                    "write": ["/tmp/mcparmor/*"]
                },
                "network": {
                    "allow":         ["api.github.com:443"],
                    "deny_local":    true,
                    "deny_metadata": true
                },
                "env":    { "allow": ["GITHUB_TOKEN", "HOME", "PATH"] },
                "output": { "scan_secrets": true, "max_size_kb": 1024 }
            }"#,
        ),
        (
            "full",
            r#"{
                "version": "1.0",
                "profile": "network",
                "network": {
                    "allow":         ["api.github.com:443", "*.googleapis.com:443", "localhost:*"],
                    "deny_local":    false,
                    "deny_metadata": true
                },
                "env":        { "allow": ["GITHUB_TOKEN", "HOME", "PATH", "NODE_PATH"] },
                "output":     { "scan_secrets": "strict", "max_size_kb": 2048 },
                "timeout_ms": 30000,
                "locked":     false,
                "min_spec":   "1.0"
            }"#,
        ),
    ];

    let mut group = c.benchmark_group("manifest_parse");
    for (name, json) in &manifests {
        group.bench_with_input(BenchmarkId::new("parse", name), json, |b, json| {
            b.iter(|| {
                serde_json::from_str::<ArmorManifest>(json).expect("manifest must parse")
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark 2: Secret scanning
// ---------------------------------------------------------------------------

/// Benchmarks `scanner::scan` across payload sizes and secret-present vs clean.
///
/// Throughput annotations allow Criterion to report MB/s alongside raw latency,
/// making it easy to verify the < 3ms (10KB) and < 15ms (100KB) budgets.
fn bench_secret_scan(c: &mut Criterion) {
    const KB_10: usize = 10 * 1024;
    const KB_100: usize = 100 * 1024;

    let clean_10kb = "x".repeat(KB_10);
    let clean_100kb = "x".repeat(KB_100);

    // Place an AWS key pattern near the end so the scanner must traverse most
    // of the payload before finding (or not finding) a match.
    let mut secret_10kb = "x".repeat(KB_10 - 20);
    secret_10kb.push_str("AKIA1234567890ABCDEF");

    let mut group = c.benchmark_group("secret_scan");

    group.throughput(Throughput::Bytes(KB_10 as u64));
    group.bench_function("clean_10kb", |b| {
        b.iter(|| scanner::scan(&clean_10kb));
    });

    group.throughput(Throughput::Bytes(KB_100 as u64));
    group.bench_function("clean_100kb", |b| {
        b.iter(|| scanner::scan(&clean_100kb));
    });

    group.throughput(Throughput::Bytes(KB_10 as u64));
    group.bench_function("secret_10kb", |b| {
        b.iter(|| scanner::scan(&secret_10kb));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion entry points
// ---------------------------------------------------------------------------

criterion_group!(benches, bench_manifest_parse, bench_secret_scan);
criterion_main!(benches);
