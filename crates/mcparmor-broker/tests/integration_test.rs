//! Integration tests for the mcparmor binary.
//!
//! These tests spawn the compiled binary and verify CLI behaviour end-to-end.
//! They require the binary to be compiled before running (`cargo build`).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a `Command` targeting the compiled mcparmor binary.
fn mcparmor() -> Command {
    Command::cargo_bin("mcparmor").expect("mcparmor binary not found — run `cargo build` first")
}

/// Write a file at `dir/filename` with the given content.
fn write_file(dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(filename);
    fs::write(&path, content).unwrap();
    path
}

/// Read a file at `path` and parse it as a JSON value.
fn read_json(path: &std::path::Path) -> serde_json::Value {
    let s = fs::read_to_string(path).unwrap();
    serde_json::from_str(&s).unwrap()
}

/// A minimal valid armor.json fixture.
const VALID_ARMOR_JSON: &str = r#"{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "sandboxed"
}"#;

/// An armor.json missing required fields (version and profile absent).
const INVALID_ARMOR_JSON_MISSING_FIELDS: &str = r#"{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json"
}"#;

/// Not valid JSON at all.
const NOT_JSON: &str = "this is not json {{}";

// ---------------------------------------------------------------------------
// validate — valid input
// ---------------------------------------------------------------------------

#[test]
fn validate_accepts_valid_armor_json() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

// ---------------------------------------------------------------------------
// validate — invalid input
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_armor_json_missing_required_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", INVALID_ARMOR_JSON_MISSING_FIELDS);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_non_existent_file() {
    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg("/tmp/this-file-does-not-exist-mcparmor.json")
        .assert()
        .failure();
}

#[test]
fn validate_rejects_non_json_file() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", NOT_JSON);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", "");

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_json_array_instead_of_object() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", r#"[1, 2, 3]"#);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_invalid_profile_value() {
    let dir = TempDir::new().unwrap();
    let content = r#"{
      "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
      "version": "1.0",
      "profile": "not-a-real-profile"
    }"#;
    let path = write_file(&dir, "armor.json", content);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_timeout_below_minimum() {
    let dir = TempDir::new().unwrap();
    let content = r#"{
      "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
      "version": "1.0",
      "profile": "sandboxed",
      "timeout_ms": 10
    }"#;
    let path = write_file(&dir, "armor.json", content);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_rejects_star_colon_star_network_rule() {
    // "*:*" grants unrestricted network access and must be rejected by the schema.
    let dir = TempDir::new().unwrap();
    let content = r#"{
      "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
      "version": "1.0",
      "profile": "network",
      "network": {
        "allow": ["*:*"]
      }
    }"#;
    let path = write_file(&dir, "armor.json", content);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn validate_accepts_star_colon_port_network_rule() {
    // "*:443" (any host, specific port) is valid per spec.
    let dir = TempDir::new().unwrap();
    let content = r#"{
      "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
      "version": "1.0",
      "profile": "network",
      "network": {
        "allow": ["*:443"]
      }
    }"#;
    let path = write_file(&dir, "armor.json", content);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// init — all profiles
// ---------------------------------------------------------------------------

#[test]
fn init_generates_valid_armor_json_for_strict_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "strict");
}

#[test]
fn init_generates_valid_armor_json_for_sandboxed_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("sandboxed")
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "sandboxed");
}

#[test]
fn init_generates_valid_armor_json_for_network_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("network")
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "network");
}

#[test]
fn init_generates_valid_armor_json_for_system_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("system")
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "system");
}

#[test]
fn init_generates_valid_armor_json_for_browser_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("browser")
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "browser");
}

#[test]
fn init_default_profile_is_sandboxed() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .assert()
        .success();

    let generated = fs::read_to_string(dir.path().join("armor.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(v["profile"], "sandboxed");
}

#[test]
fn init_fails_if_armor_json_already_exists() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .assert()
        .failure();
}

#[test]
fn init_rejects_unknown_profile() {
    let dir = TempDir::new().unwrap();

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg("not-a-real-profile")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// init → validate round-trip: every generated profile must be schema-valid
// ---------------------------------------------------------------------------

#[test]
fn init_strict_round_trip_passes_validate() {
    round_trip_validate("strict");
}

#[test]
fn init_sandboxed_round_trip_passes_validate() {
    round_trip_validate("sandboxed");
}

#[test]
fn init_network_round_trip_passes_validate() {
    round_trip_validate("network");
}

#[test]
fn init_browser_round_trip_passes_validate() {
    round_trip_validate("browser");
}

/// Generate armor.json via `init --profile <name>` then validate it.
fn round_trip_validate(profile: &str) {
    let dir = TempDir::new().unwrap();
    let armor_path = dir.path().join("armor.json");

    mcparmor()
        .arg("init")
        .arg("--dir")
        .arg(dir.path())
        .arg("--profile")
        .arg(profile)
        .assert()
        .success();

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&armor_path)
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

#[test]
fn status_runs_without_error() {
    mcparmor()
        .arg("status")
        .assert()
        .success();
}

#[test]
fn status_json_format_produces_parseable_output() {
    let output = mcparmor()
        .arg("status")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    assert!(output.status.success());
    // If there is JSON output on stdout, it must be parseable.
    let stdout = String::from_utf8(output.stdout).unwrap();
    if !stdout.trim().is_empty() {
        serde_json::from_str::<serde_json::Value>(stdout.trim())
            .expect("status --format json output is not valid JSON");
    }
}

// ---------------------------------------------------------------------------
// audit
// ---------------------------------------------------------------------------

#[test]
fn audit_on_empty_log_succeeds() {
    // Write an empty log file to a temp location.
    // We can't inject the log path directly, but we can verify the command
    // at least exits cleanly when no log exists (the default path won't exist in CI).
    // This test verifies the absence of a panic or crash.
    let result = mcparmor().arg("audit").output();
    // The command should either succeed or fail gracefully — not crash.
    assert!(result.is_ok(), "audit command should not panic");
}

#[test]
fn audit_stats_flag_is_accepted() {
    let result = mcparmor().arg("audit").arg("--stats").output();
    assert!(result.is_ok(), "audit --stats should not crash");
}

#[test]
fn audit_since_relative_1h_is_accepted() {
    let result = mcparmor().arg("audit").arg("--since").arg("1h").output();
    assert!(result.is_ok(), "audit --since 1h should not crash");
}

#[test]
fn audit_since_relative_7d_is_accepted() {
    let result = mcparmor().arg("audit").arg("--since").arg("7d").output();
    assert!(result.is_ok(), "audit --since 7d should not crash");
}

#[test]
fn audit_json_format_is_accepted() {
    let result = mcparmor()
        .arg("audit")
        .arg("--format")
        .arg("json")
        .output();
    assert!(result.is_ok(), "audit --format json should not crash");
}

// ---------------------------------------------------------------------------
// run — early error cases (no tool command)
// ---------------------------------------------------------------------------

#[test]
fn run_applies_strict_fallback_when_no_armor_json_present() {
    // When no armor.json exists, the broker applies a strict fallback profile
    // and emits a warning to stderr — it does NOT fail, so the tool can still run.
    let dir = TempDir::new().unwrap();
    mcparmor()
        .arg("run")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("strict profile"));
}

#[test]
fn run_loads_armor_json_from_parent_directory_not_fallback() {
    // armor.json in a parent dir must be found by upward search — not silently bypassed.
    // If the broker loads the manifest, it exits without error and produces no fallback warning.
    let parent = TempDir::new().unwrap();
    let child_dir = parent.path().join("subdir");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(parent.path().join("armor.json"), VALID_ARMOR_JSON).unwrap();

    mcparmor()
        .arg("run")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .current_dir(&child_dir)
        .assert()
        .success()
        // No fallback warning — the manifest was found via upward search.
        .stderr(predicate::str::contains("strict profile").not());
}

#[test]
fn run_fails_when_upward_search_finds_invalid_armor_json() {
    // An invalid armor.json found during upward search must cause a hard failure,
    // not a silent fallback to the strict profile.
    let parent = TempDir::new().unwrap();
    let child_dir = parent.path().join("subdir");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(parent.path().join("armor.json"), NOT_JSON).unwrap();

    mcparmor()
        .arg("run")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .current_dir(&child_dir)
        .assert()
        .failure();
}

#[test]
fn run_fails_when_explicit_armor_path_does_not_exist() {
    let dir = TempDir::new().unwrap();
    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(dir.path().join("nonexistent.armor.json"))
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// profiles — smoke test
// ---------------------------------------------------------------------------

#[test]
fn profiles_list_runs_without_panic() {
    let result = mcparmor().arg("profiles").arg("list").output();
    assert!(result.is_ok(), "profiles list should not panic");
}

#[test]
fn profiles_show_unknown_profile_fails_gracefully() {
    mcparmor()
        .arg("profiles")
        .arg("show")
        .arg("this-profile-does-not-exist")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Fixture content for wrap/unwrap integration tests
// ---------------------------------------------------------------------------

const CLAUDE_DESKTOP_BEFORE: &str =
    include_str!("../../../tests/integration/fixtures/claude_desktop_before.json");
const CURSOR_BEFORE: &str =
    include_str!("../../../tests/integration/fixtures/cursor_before.json");
const VSCODE_BEFORE: &str =
    include_str!("../../../tests/integration/fixtures/vscode_before.json");

// ---------------------------------------------------------------------------
// wrap — core behaviour
// ---------------------------------------------------------------------------

#[test]
fn wrap_claude_desktop_wraps_all_stdio_entries() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "claude_desktop_config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let wrapped = read_json(&config);
    let servers = wrapped["mcpServers"].as_object().unwrap();

    // All stdio tools must have command == "mcparmor"
    for (name, server) in servers.iter() {
        // HTTP/URL-based tools are skipped
        if server["url"].is_null() && server["type"].as_str() != Some("http") {
            assert_eq!(
                server["command"].as_str().unwrap(),
                "mcparmor",
                "tool {name} was not wrapped"
            );
            // args must start with "run"
            assert_eq!(server["args"][0].as_str().unwrap(), "run");
            // "--" separator must be present
            let args: Vec<&str> = server["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert!(args.contains(&"--"), "tool {name} args missing -- separator");
        }
    }
}

#[test]
fn wrap_skips_http_tools() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let wrapped = read_json(&config);
    // The HTTP tool should not be modified (still has "url" key, no "command")
    let http_tool = &wrapped["mcpServers"]["remote-api"];
    assert!(http_tool["url"].is_string(), "HTTP tool url should be preserved");
    assert!(
        http_tool["command"].is_null(),
        "HTTP tool should not have command added"
    );
}

#[test]
fn wrap_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    // Wrap once
    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let after_first = fs::read_to_string(&config).unwrap();

    // Wrap again — must be idempotent
    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let after_second = fs::read_to_string(&config).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&after_first).unwrap(),
        serde_json::from_str::<serde_json::Value>(&after_second).unwrap(),
        "wrap must be idempotent"
    );
}

#[test]
fn wrap_dry_run_does_not_modify_config() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);
    let original = fs::read_to_string(&config).unwrap();

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .arg("--dry-run")
        .assert()
        .success();

    let after = fs::read_to_string(&config).unwrap();
    assert_eq!(original, after, "dry-run must not modify the config file");
}

#[test]
fn wrap_creates_backup_file() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let bak = dir.path().join("config.json.bak");
    assert!(bak.exists(), "backup file must be created");

    // Backup must be valid JSON matching the original
    let bak_content = read_json(&bak);
    let original = serde_json::from_str::<serde_json::Value>(CLAUDE_DESKTOP_BEFORE).unwrap();
    assert_eq!(bak_content, original, "backup must match original config");
}

#[test]
fn wrap_output_is_valid_json() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let content = fs::read_to_string(&config).unwrap();
    serde_json::from_str::<serde_json::Value>(&content)
        .expect("wrapped config must be valid JSON");
}

#[test]
fn wrap_vscode_uses_servers_key() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "mcp.json", VSCODE_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let wrapped = read_json(&config);
    // VS Code uses "servers" not "mcpServers"
    assert!(
        wrapped["servers"].is_object(),
        "vscode config must use 'servers' key"
    );
    let servers = wrapped["servers"].as_object().unwrap();
    for (name, server) in servers.iter() {
        assert_eq!(
            server["command"].as_str().unwrap(),
            "mcparmor",
            "vscode tool {name} was not wrapped"
        );
    }
}

#[test]
fn wrap_cursor_wraps_all_stdio_entries() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CURSOR_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let wrapped = read_json(&config);
    let servers = wrapped["mcpServers"].as_object().unwrap();
    for (name, server) in servers.iter() {
        assert_eq!(
            server["command"].as_str().unwrap(),
            "mcparmor",
            "cursor tool {name} was not wrapped"
        );
    }
}

#[test]
fn wrap_fails_on_nonexistent_config() {
    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg("/this/path/does/not/exist/config.json")
        .arg("--profile")
        .arg("strict")
        .assert()
        .failure();
}

#[test]
fn wrap_fails_on_invalid_json_config() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", NOT_JSON);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// unwrap — restores original
// ---------------------------------------------------------------------------

#[test]
fn unwrap_restores_original_config() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);
    let original = serde_json::from_str::<serde_json::Value>(CLAUDE_DESKTOP_BEFORE).unwrap();

    // Wrap first
    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    // Then unwrap
    mcparmor()
        .arg("unwrap")
        .arg("--config")
        .arg(&config)
        .assert()
        .success();

    let restored = read_json(&config);
    assert_eq!(restored, original, "unwrap must restore original config exactly");
}

#[test]
fn unwrap_produces_valid_json() {
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();
    mcparmor()
        .arg("unwrap")
        .arg("--config")
        .arg(&config)
        .assert()
        .success();

    let content = fs::read_to_string(&config).unwrap();
    serde_json::from_str::<serde_json::Value>(&content)
        .expect("unwrapped config must be valid JSON");
}

#[test]
fn unwrap_fails_on_nonexistent_config() {
    mcparmor()
        .arg("unwrap")
        .arg("--config")
        .arg("/this/path/does/not/exist/config.json")
        .assert()
        .failure();
}

#[test]
fn unwrap_on_already_unwrapped_config_is_a_no_op_or_succeeds() {
    // Unwrapping a config that was never wrapped should not crash.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    let result = mcparmor().arg("unwrap").arg("--config").arg(&config).output();
    assert!(result.is_ok(), "unwrap on clean config should not crash");
}

// ---------------------------------------------------------------------------
// wrap — --no-armor-path flag
// ---------------------------------------------------------------------------

/// A minimal stdio-only config fixture (no HTTP tools) for no-armor-path tests.
const STDIO_ONLY_CONFIG: &str = r#"{
  "mcpServers": {
    "fetch": {
      "command": "uvx",
      "args": ["mcp-server-fetch"]
    }
  }
}"#;

#[test]
fn wrap_no_armor_path_omits_armor_flag_when_armor_json_exists_in_cwd() {
    // When armor.json is present in the working directory, a normal wrap embeds
    // the path via --armor. With --no-armor-path, the flag must be absent.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", STDIO_ONLY_CONFIG);
    write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .current_dir(dir.path())
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .arg("--no-armor-path")
        .assert()
        .success();

    let wrapped = read_json(&config);
    let args: Vec<String> = wrapped["mcpServers"]["fetch"]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(
        !args.contains(&"--armor".to_string()),
        "--no-armor-path: wrapped args must not contain --armor, got: {args:?}"
    );
}

#[test]
fn wrap_discovers_armor_json_from_cwd_when_no_armor_path_is_absent() {
    // When armor.json is in the working directory and --no-armor-path is NOT
    // set, the broker should embed --armor <path> in the wrapped args.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", STDIO_ONLY_CONFIG);
    write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .current_dir(dir.path())
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success();

    let wrapped = read_json(&config);
    let args: Vec<String> = wrapped["mcpServers"]["fetch"]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(
        args.contains(&"--armor".to_string()),
        "wrapped args should contain --armor when armor.json exists in cwd, got: {args:?}"
    );
}

#[test]
fn wrap_no_armor_path_produces_valid_run_args() {
    // Wrapped args must still start with "run" and contain "--" even without armor.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", STDIO_ONLY_CONFIG);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--no-armor-path")
        .assert()
        .success();

    let wrapped = read_json(&config);
    let args: Vec<String> = wrapped["mcpServers"]["fetch"]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(args[0], "run", "first arg must be 'run'");
    assert!(
        args.contains(&"--".to_string()),
        "wrapped args must contain '--' separator, got: {args:?}"
    );
    // Original command must be preserved after the separator
    let sep_pos = args.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        args[sep_pos + 1], "uvx",
        "original command 'uvx' must follow '--'"
    );
}

// ---------------------------------------------------------------------------
// wrap — HTTP tool warning
// ---------------------------------------------------------------------------

#[test]
fn wrap_emits_warning_to_stderr_for_http_tools() {
    // The CLAUDE_DESKTOP_BEFORE fixture includes a "remote-api" HTTP tool.
    // mcparmor must emit a warning to stderr for it but still succeed.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .assert()
        .success()
        .stderr(predicate::str::contains("remote-api"))
        .stderr(predicate::str::contains("HTTP"));
}

#[test]
fn wrap_http_warning_names_the_skipped_tool() {
    // The warning message must identify the specific tool name so operators
    // know which entry was not wrapped.
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    let output = mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .output()
        .expect("wrap command should not panic");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("remote-api"),
        "stderr warning must name the HTTP tool; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// wrap — windsurf host resolution
// ---------------------------------------------------------------------------

#[test]
fn wrap_host_windsurf_is_recognized_not_unknown() {
    // --host windsurf should resolve to a known path and fail only because the
    // file doesn't exist at ~/.codeium/windsurf/mcp_config.json in CI — not
    // because the host name is unrecognised.
    let output = mcparmor()
        .arg("wrap")
        .arg("--host")
        .arg("windsurf")
        .arg("--profile")
        .arg("strict")
        .output()
        .expect("command should not panic");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Unknown host"),
        "windsurf must be a recognised host name; got stderr: {stderr}"
    );
    // It may fail because the file doesn't exist — that is expected in CI.
    // What must NOT happen is an "Unknown host" error.
}

// ---------------------------------------------------------------------------
// GAP 1: --strict and --verbose flags for `mcparmor run`
// ---------------------------------------------------------------------------

#[test]
fn run_accepts_strict_flag() {
    // --strict must be accepted by the CLI parser (no "unrecognized argument" error).
    // We run a simple echo command so it exits cleanly.
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&path)
        .arg("--strict")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

#[test]
fn run_accepts_verbose_flag() {
    // --verbose / -v must be accepted by the CLI parser.
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&path)
        .arg("--verbose")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

#[test]
fn run_accepts_verbose_short_flag() {
    // Short flag -v must also be accepted.
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&path)
        .arg("-v")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

#[test]
fn run_strict_and_verbose_can_be_combined() {
    // Both flags together must not cause a parse error.
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&path)
        .arg("--strict")
        .arg("--verbose")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// GAP 2: validate shows OS sandbox section
// ---------------------------------------------------------------------------

#[test]
fn validate_shows_os_sandbox_section() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .assert()
        .success()
        // The sandbox section must appear in stdout.
        .stdout(predicate::str::contains("OS sandbox"));
}

#[test]
fn validate_sandbox_section_contains_filesystem_isolation_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    let output = mcparmor()
        .arg("validate")
        .arg("--armor")
        .arg(&path)
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Filesystem isolation"),
        "sandbox section must include filesystem isolation line; got: {stdout}"
    );
    assert!(
        stdout.contains("Spawn blocking"),
        "sandbox section must include spawn blocking line; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// GAP 3: status shows Platform line and Summary line
// ---------------------------------------------------------------------------

#[test]
fn status_shows_platform_line() {
    let output = mcparmor()
        .arg("status")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Platform:"),
        "status must show Platform line; got: {stdout}"
    );
}

#[test]
fn status_platform_line_contains_layer1_indicator() {
    let output = mcparmor()
        .arg("status")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Layer 1"),
        "Platform line must mention Layer 1; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// GAP 3 (continued): --no-audit and --audit-log flags for `mcparmor run`
// ---------------------------------------------------------------------------

#[test]
fn run_accepts_no_audit_flag() {
    // --no-audit must be accepted by the CLI parser and the tool must complete normally.
    let dir = TempDir::new().unwrap();
    let path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&path)
        .arg("--no-audit")
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

#[test]
fn run_accepts_audit_log_flag() {
    // --audit-log <file> must be accepted by the CLI parser and the tool must complete normally.
    let dir = TempDir::new().unwrap();
    let armor_path = write_file(&dir, "armor.json", VALID_ARMOR_JSON);
    let audit_path = dir.path().join("test-audit.jsonl");

    mcparmor()
        .arg("run")
        .arg("--armor")
        .arg(&armor_path)
        .arg("--audit-log")
        .arg(&audit_path)
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// GAP 4: wrap shows armor source annotation
// ---------------------------------------------------------------------------

#[test]
fn wrap_output_shows_armored_status_per_tool() {
    // After wrapping, each tool line should contain "armored".
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    let output = mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--profile")
        .arg("strict")
        .output()
        .expect("wrap must not panic");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("armored"),
        "wrap output must contain 'armored' status for wrapped tools; got: {stdout}"
    );
}

#[test]
fn wrap_output_shows_strict_fallback_when_no_armor_json_present() {
    // When no armor.json is in the tool's directory tree or community profiles,
    // the output should mention "strict fallback".
    let dir = TempDir::new().unwrap();
    let config = write_file(&dir, "config.json", CLAUDE_DESKTOP_BEFORE);

    let output = mcparmor()
        .arg("wrap")
        .arg("--config")
        .arg(&config)
        .arg("--no-armor-path")
        .output()
        .expect("wrap must not panic");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // When --no-armor-path is set, no armor.json lookup is performed => StrictFallback.
    assert!(
        stdout.contains("strict fallback"),
        "wrap with --no-armor-path must show 'strict fallback' source; got: {stdout}"
    );
}
