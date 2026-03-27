# MCP Armor — Technical Execution Plan

> Architect: Arqitect team
> Stack: Rust (core/broker/CLI), Python SDK, Node SDK
> Target: v1 shipped alongside Arqitect launch

---

## Repository Structure

```
mcparmor/
├── Cargo.toml                        ← workspace manifest
├── crates/
│   ├── mcparmor-core/                ← types, manifest parsing, secret scanner
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manifest.rs           ← ArmorManifest, parse + validate
│   │       ├── policy.rs             ← capability enforcement decisions
│   │       ├── scanner.rs            ← secret/PII output scanning
│   │       └── audit.rs              ← AuditLog, AuditEvent types
│   ├── mcparmor-broker/              ← stdio proxy process
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── proxy.rs              ← JSON-RPC stdio proxy loop
│   │       ├── interceptor.rs        ← param inspection, path/host validation
│   │       └── trampoline.rs         ← Python trampoline injection
│   └── mcparmor-cli/                 ← the `mcparmor` binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
├── spec/
│   └── armor-manifest.schema.json    ← THE canonical schema
├── sdks/
│   ├── python/
│   │   ├── pyproject.toml
│   │   ├── mcparmor/
│   │   │   ├── __init__.py
│   │   │   ├── tool.py               ← ArmoredTool class
│   │   │   ├── manifest.py           ← manifest loader
│   │   │   └── trampoline.py         ← Python capability interceptor
│   │   └── tests/
│   └── node/
│       ├── package.json
│       ├── src/
│       │   ├── index.ts
│       │   ├── tool.ts               ← ArmoredTool class
│       │   └── manifest.ts
│       └── tests/
├── profiles/
│   └── community/                    ← armor profiles for popular MCP tools
│       ├── github.armor.json
│       ├── filesystem.armor.json
│       └── ...
├── docs/
│   ├── getting-started.md
│   ├── manifest-spec.md
│   ├── integrations/
│   │   ├── arqitect.md
│   │   ├── openclaw.md
│   │   └── nanoclaw.md
│   └── security-model.md
└── tests/
    ├── adversarial/                  ← tools that try to escape
    │   ├── read_passwd.py
    │   ├── call_forbidden_host.py
    │   ├── leak_secret_output.py
    │   └── spawn_child.py
    └── fixtures/
        └── tools/
```

---

## M0 — Armor Manifest Spec

### Decision: Extend existing `tool.json`, don't invent a new file

The `armor` block lives inside the existing tool manifest. Tool authors add one block. No new file format to learn.

### Schema Design

```json
{
  "armor": {
    "version": "1.0",
    "profile": "sandboxed",

    "filesystem": {
      "read": ["/tmp/mcparmor/*"],
      "write": ["/tmp/mcparmor/*"],
      "deny_home": true
    },

    "network": {
      "allow": ["api.github.com:443", "*.googleapis.com:443"],
      "deny_local": true,
      "deny_metadata": true
    },

    "spawn": false,

    "env": {
      "allow": ["GITHUB_TOKEN", "HOME", "PATH"],
      "deny_system": true
    },

    "output": {
      "scan_secrets": true,
      "max_bytes": 1048576
    },

    "resources": {
      "timeout_ms": 30000,
      "max_memory_mb": 256
    }
  }
}
```

### Profile Presets (shorthand for common patterns)

```json
{ "armor": { "profile": "strict" } }
```

| Profile | filesystem | network | spawn | env | Use case |
|---|---|---|---|---|---|
| `strict` | none | none | false | none | Dream-state / fabricated tools |
| `sandboxed` | `/tmp/mcparmor/*` r/w | declared only | false | declared only | Community tools (default) |
| `network` | none | declared only | false | declared only | Pure API tools |
| `system` | declared paths | declared hosts | false | declared | System/OS tools |
| `browser` | `/tmp/mcparmor/*` r/w | `*:443` | false | declared | Browser automation tools |

Profiles are sugar. Full override always available.

### Real Examples

**cert_check** (network only, no filesystem):
```json
"armor": {
  "profile": "network",
  "network": { "allow": ["*:443"], "deny_local": true }
}
```

**browser_click** (needs Playwright IPC):
```json
"armor": {
  "profile": "browser",
  "filesystem": {
    "read": ["~/.arqitect_browser_cdp.json", "~/.arqitect_browser_pages.json"]
  },
  "network": { "allow": ["localhost:*"], "deny_metadata": true }
}
```

**barcode** (filesystem I/O, no network):
```json
"armor": {
  "profile": "sandboxed",
  "filesystem": {
    "read": ["/tmp/mcparmor/*", "$input_path"],
    "write": ["/tmp/mcparmor/*", "$output_path"]
  },
  "network": { "allow": [] }
}
```

**Fabricated tool** (hardcoded strict, non-overridable):
```json
"armor": {
  "profile": "strict",
  "locked": true
}
```

`locked: true` means the MCP host cannot relax this profile at runtime.

### JSON Schema (`armor-manifest.schema.json`)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://mcparmor.io/spec/1.0/armor-manifest.schema.json",
  "title": "MCP Armor Manifest",
  "type": "object",
  "required": ["version"],
  "properties": {
    "version":  { "type": "string", "enum": ["1.0"] },
    "profile":  { "type": "string", "enum": ["strict","sandboxed","network","system","browser"] },
    "locked":   { "type": "boolean", "default": false },
    "filesystem": {
      "type": "object",
      "properties": {
        "read":       { "type": "array", "items": { "type": "string" } },
        "write":      { "type": "array", "items": { "type": "string" } },
        "deny_home":  { "type": "boolean", "default": true }
      }
    },
    "network": {
      "type": "object",
      "properties": {
        "allow":          { "type": "array", "items": { "type": "string" } },
        "deny_local":     { "type": "boolean", "default": true },
        "deny_metadata":  { "type": "boolean", "default": true }
      }
    },
    "spawn":  { "type": "boolean", "default": false },
    "env": {
      "type": "object",
      "properties": {
        "allow":        { "type": "array", "items": { "type": "string" } },
        "deny_system":  { "type": "boolean", "default": true }
      }
    },
    "output": {
      "type": "object",
      "properties": {
        "scan_secrets": { "type": "boolean", "default": true },
        "max_bytes":    { "type": "integer", "default": 1048576 }
      }
    },
    "resources": {
      "type": "object",
      "properties": {
        "timeout_ms":     { "type": "integer", "default": 30000 },
        "max_memory_mb":  { "type": "integer", "default": 256 }
      }
    }
  }
}
```

---

## M1 — Broker Architecture

### Core Decision: Stdio Proxy + Python Trampoline

Two enforcement layers:

**Layer 1 — Stdio proxy (all tools, all languages)**
The broker sits between the MCP host and the tool subprocess. Every JSON-RPC message flows through it. The broker inspects params (paths, URLs) before forwarding to the tool, and scans responses for secrets before returning.

**Layer 2 — Python trampoline (Python tools only)**
For Python tools, the broker injects a trampoline script that runs before the tool code. The trampoline monkey-patches the Python runtime: `open()`, `requests`, `socket`, `subprocess`, `os.environ`. Any call that violates the manifest raises a `CapabilityViolation` — which the broker catches, logs, and returns as a JSON-RPC error.

This covers the full Arqitect mcp_tools library since all tools are Python or Rust. Rust tools get Layer 1 only (protocol-level) — sufficient for v1.

### Process Lifecycle

```
1. MCP Host spawns: mcparmor run --manifest tool.json -- python tool.py
2. Broker reads + validates armor manifest
3. Broker spawns tool subprocess with modified environment:
   - MCPARMOR_MANIFEST=/path/to/tool.json
   - MCPARMOR_CALL_ID=<uuid>
   - For Python: PYTHONPATH prepended with trampoline dir
4. Broker enters proxy loop:
   a. Read JSON-RPC line from MCP Host stdin
   b. Validate params against manifest (paths, URLs in param values)
   c. Forward approved request to tool subprocess stdin
   d. Read JSON-RPC response from tool subprocess stdout
   e. Scan response for secrets
   f. Log the full event
   g. Forward clean response to MCP Host stdout
5. On timeout/error: kill tool subprocess, return JSON-RPC error, flush audit log
```

### Interceptor Logic

**Filesystem interception (Python trampoline):**
```python
# injected before tool code runs
import builtins
_original_open = builtins.open
_allowed_reads = ["/tmp/mcparmor/*"]
_allowed_writes = ["/tmp/mcparmor/*"]

def _armored_open(path, mode="r", **kwargs):
    resolved = os.path.realpath(path)  # resolve symlinks
    if "w" in mode or "a" in mode:
        if not _matches_any(resolved, _allowed_writes):
            raise CapabilityViolation(f"write denied: {resolved}")
    else:
        if not _matches_any(resolved, _allowed_reads):
            raise CapabilityViolation(f"read denied: {resolved}")
    return _original_open(path, mode, **kwargs)

builtins.open = _armored_open
```

**Network interception (Python trampoline):**
```python
import socket
_original_getaddrinfo = socket.getaddrinfo

def _armored_getaddrinfo(host, port, *args, **kwargs):
    if not _host_allowed(host, port):
        raise CapabilityViolation(f"network denied: {host}:{port}")
    return _original_getaddrinfo(host, port, *args, **kwargs)

socket.getaddrinfo = _armored_getaddrinfo
```

Patching `socket.getaddrinfo` covers all network libraries: `requests`, `httpx`, `urllib`, raw sockets — everything goes through DNS resolution first.

**Spawn interception (Python trampoline):**
```python
import subprocess as _sp
def _denied_spawn(*args, **kwargs):
    raise CapabilityViolation("spawn denied")

if not _armor_allows_spawn():
    _sp.Popen = _denied_spawn
    _sp.run = _denied_spawn
    _sp.call = _denied_spawn
```

### Secret Scanner

Extend Arqitect's `check_secrets.py` patterns, compiled as Rust regex set for performance:

```rust
pub const SECRET_PATTERNS: &[(&str, &str)] = &[
    (r"sk-[A-Za-z0-9]{20,}", "openai_key"),
    (r"ghp_[A-Za-z0-9]{36,}", "github_pat"),
    (r"AKIA[A-Z0-9]{16}", "aws_access_key"),
    (r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----", "private_key"),
    (r"(?:mongodb|postgres|mysql)://[^\s\"']+:[^\s\"']+@", "db_connection"),
    (r#"(?i)(?:api.?key|secret|password|token)\s*[=:]\s*["']?[A-Za-z0-9\-._~+/]{16,}"#, "generic_secret"),
    (r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+", "jwt_token"),
];
```

Scanner runs on every response before forwarding. If a secret is found:
1. Log the detection event
2. Redact the value in the response (`[REDACTED:openai_key]`)
3. If `scan_secrets: strict` — return a JSON-RPC error instead of redacting

### Audit Log Format (JSONL)

One JSON object per line, append-only, written to `~/.mcparmor/audit/<date>.jsonl`:

```jsonl
{"ts":"2026-03-27T10:00:00.123Z","session":"abc123","tool":"cert_check","event":"invoke","call_id":"uuid1","params":{"domain":"example.com"}}
{"ts":"2026-03-27T10:00:00.456Z","session":"abc123","tool":"cert_check","event":"network_check","call_id":"uuid1","host":"example.com","port":443,"allowed":true}
{"ts":"2026-03-27T10:00:01.789Z","session":"abc123","tool":"cert_check","event":"response","call_id":"uuid1","secret_scan":"clean","latency_ms":1666,"bytes":342}
{"ts":"2026-03-27T10:00:02.000Z","session":"abc123","tool":"browser_click","event":"fs_violation","call_id":"uuid2","path":"/etc/passwd","profile":"sandboxed","action":"blocked"}
```

---

## M2 — CLI Design

### Commands

```
mcparmor run     Run a tool under armor
mcparmor validate  Validate an armor manifest
mcparmor audit   Query the audit log
mcparmor init    Add armor block to an existing tool.json
mcparmor profile List built-in profiles
```

### `mcparmor run`

```
USAGE:
  mcparmor run [OPTIONS] --manifest <FILE> -- <COMMAND> [ARGS...]

OPTIONS:
  -m, --manifest <FILE>     Path to tool.json containing armor block
  -p, --profile <PROFILE>   Override profile (cannot override if locked: true)
  --audit-log <FILE>         Audit log path [default: ~/.mcparmor/audit/today.jsonl]
  --no-audit                 Disable audit logging
  --strict                   Treat any capability violation as fatal (exit 1)
  -v, --verbose              Print capability checks to stderr

EXIT CODES:
  0   Success
  1   Tool error (JSON-RPC error response)
  2   Capability violation (blocked by armor)
  3   Manifest invalid
  4   Timeout
```

### `mcparmor validate`

```
USAGE:
  mcparmor validate --manifest <FILE>

OUTPUT:
  ✓ Manifest valid — profile: sandboxed
    filesystem: read [/tmp/mcparmor/*], write [/tmp/mcparmor/*]
    network: allow [api.github.com:443]
    spawn: false
    output scan: enabled

  ✗ Manifest invalid
    line 12: "profile" must be one of: strict, sandboxed, network, system, browser
```

### `mcparmor audit`

```
USAGE:
  mcparmor audit [OPTIONS]

OPTIONS:
  --tool <NAME>       Filter by tool name
  --event <TYPE>      Filter by event type (invoke|violation|secret_detected)
  --since <DATETIME>  Filter by timestamp
  --format <FORMAT>   Output format: table (default) | json | jsonl

EXAMPLE OUTPUT:
  TIME                  TOOL         EVENT            DETAIL
  2026-03-27 10:00:00   cert_check   invoke           domain=example.com
  2026-03-27 10:00:01   cert_check   network_check    example.com:443 ✓
  2026-03-27 10:00:01   cert_check   response         clean, 342 bytes, 1666ms
  2026-03-27 10:00:05   browser_clk  fs_violation     /etc/passwd BLOCKED
```

### `mcparmor init`

```
USAGE:
  mcparmor init --tool-dir <DIR>

Analyzes the tool's code, suggests an armor block, writes it to tool.json.
Interactive — confirms each capability before writing.

  Analyzing tool.py...
  → Detected: network calls to *.example.com
  → Detected: filesystem reads in /tmp
  → No subprocess spawning detected

  Suggested armor profile: sandboxed
  Network allow: ["*.example.com:443"]
  Filesystem read: ["/tmp/mcparmor/*"]

  Write to tool.json? [Y/n]
```

---

## M3 — Python SDK

### API Surface

```python
from mcparmor import ArmoredTool, ArmorManifest, CapabilityViolation

# --- Basic usage ---
tool = ArmoredTool(
    manifest_path="tool.json",
    command=["python", "tool.py"],
    tool_dir="/path/to/tool"
)

# Single invocation
result = tool.invoke({"domain": "example.com"})

# Context manager (persistent subprocess, multiple calls)
with ArmoredTool("tool.json", ["python", "tool.py"]) as tool:
    r1 = tool.invoke({"domain": "example.com"})
    r2 = tool.invoke({"domain": "google.com"})

# --- Arqitect ToolManager integration ---
from mcparmor import armor_subprocess

# Drop-in replacement for subprocess.Popen in ToolManager
process = armor_subprocess(
    command=["python", "tool.py"],
    manifest_path="tool.json",
    cwd=tool_dir
)
# process speaks same JSON-RPC stdio interface

# --- Manifest inspection ---
manifest = ArmorManifest.load("tool.json")
print(manifest.profile)          # "sandboxed"
print(manifest.allows_network("api.github.com", 443))  # True
print(manifest.allows_path_read("/etc/passwd"))         # False
print(manifest.is_locked())      # False
```

### Arqitect ToolManager Integration

```python
# In arqitect-server ToolManager — minimal change required
from mcparmor import armor_subprocess

def _spawn_subprocess(self, tool: ToolConfig) -> subprocess.Popen:
    manifest_path = os.path.join(tool.dir, "tool.json")
    armor = ArmorManifest.load(manifest_path)

    if armor.has_armor_block():
        return armor_subprocess(
            command=tool.command,
            manifest_path=manifest_path,
            cwd=tool.dir,
            env=self._build_env(tool)
        )

    # fallback: unarmored (legacy tools)
    return subprocess.Popen(tool.command, ...)
```

### Packaging

```
pip install mcparmor
```

- Pure Python wrapper around the Rust binary
- Ships the `mcparmor` binary for the current platform as a package data file
- Auto-downloads correct binary on install via `pip` if platform binary not bundled
- Minimum Python: 3.10

---

## M4 — Node SDK

### API Surface

```typescript
import { ArmoredTool, ArmorManifest } from 'mcparmor';

// Basic usage
const tool = new ArmoredTool({
  manifestPath: 'tool.json',
  command: ['node', 'tool.js'],
  toolDir: '/path/to/tool'
});

const result = await tool.invoke({ query: 'hello' });
await tool.close();

// Stream mode (persistent subprocess)
const tool = await ArmoredTool.spawn('tool.json', ['node', 'tool.js']);
const result = await tool.invoke(params);
await tool.close();

// Manifest inspection
const manifest = ArmorManifest.load('tool.json');
manifest.allowsNetwork('api.github.com', 443); // true
manifest.allowsPathRead('/etc/passwd');         // false
```

### OpenClaw / NanoClaw Integration Pattern

```typescript
// Drop-in for any OpenClaw skill runner
import { armorSpawn } from 'mcparmor';

const proc = armorSpawn({
  command: ['python', 'tool.py'],
  manifestPath: 'tool.json'
});

// proc is a standard ChildProcess — existing code unchanged
proc.stdin.write(JSON.stringify(jsonRpcRequest) + '\n');
```

---

## Cross-Platform Strategy

### Layer 1 — Broker (all platforms, always active)
Protocol-level enforcement. Works everywhere with zero OS dependencies. This is v1.

### Layer 2 — OS Primitives (v2, Linux + macOS first)

Generated automatically from the armor manifest. Belt-and-suspenders — not a replacement for Layer 1.

**Linux (Seccomp + Landlock):**
```rust
// mcparmor-broker generates at spawn time
fn apply_linux_profile(manifest: &ArmorManifest) -> Result<()> {
    // Landlock: restrict filesystem access
    let ruleset = LandlockRuleset::new()?;
    for path in manifest.filesystem.read.iter() {
        ruleset.add_rule(LandlockRule::PathBeneath {
            path, access: AccessFs::READ
        })?;
    }
    ruleset.restrict_self()?;

    // Seccomp: block spawn if not allowed
    if !manifest.spawn {
        let filter = SeccompFilter::new(SeccompAction::Errno(EPERM));
        filter.add_rule(SeccompRule::new(Syscall::Execve, SeccompAction::Kill));
        filter.load()?;
    }
    Ok(())
}
```

**macOS (Seatbelt):**
```rust
// Generate Seatbelt profile string from manifest
fn generate_seatbelt_profile(manifest: &ArmorManifest) -> String {
    let mut rules = vec!["(version 1)", "(deny default)"];
    for path in manifest.filesystem.read.iter() {
        rules.push(&format!("(allow file-read* (subpath \"{path}\"))"));
    }
    // spawn
    if !manifest.spawn {
        rules.push("(deny process-exec)");
    }
    rules.join("\n")
}

// Then: sandbox_init(profile, 0, &err)
```

**Windows (v2, lower priority):**
AppContainer via Win32 API. Same manifest → AppContainer capability set.

---

## Testing Strategy

### Unit Tests (mcparmor-core)
- Manifest parsing: valid, missing fields, unknown profiles, locked override attempts
- Policy decisions: `allows_path_read("/etc/passwd")` → false, `allows_path_read("/tmp/mcparmor/foo")` → true
- Secret scanner: each pattern type, false positive rate, redaction output
- Path glob matching: wildcards, symlink resolution, relative paths

### Integration Tests (mcparmor-broker)
Run real tool subprocesses through the broker and assert behavior:

```
tests/adversarial/read_passwd.py      → expect: CapabilityViolation, exit code 2
tests/adversarial/call_forbidden.py   → expect: CapabilityViolation, exit code 2
tests/adversarial/leak_secret.py      → expect: response with [REDACTED:openai_key]
tests/adversarial/spawn_child.py      → expect: CapabilityViolation, exit code 2
tests/adversarial/timeout.py          → expect: exit code 4 after 30s
```

### Fixture Tools (legitimate, should pass)
```
tests/fixtures/tools/cert_check/      → network call to real host → should succeed
tests/fixtures/tools/base64/          → pure compute, no I/O → should succeed
tests/fixtures/tools/file_write/      → writes to /tmp/mcparmor → should succeed
```

### CI Matrix

| OS | Python | Node | Test suite |
|---|---|---|---|
| ubuntu-latest | 3.10, 3.12 | 20, 22 | full |
| macos-latest | 3.12 | 22 | full |
| windows-latest | 3.12 | 22 | broker + CLI only (no OS primitives) |

### Property-Based Tests
Use `proptest` (Rust) for manifest parsing — generate random JSON, assert no panics.

---

## Launch Checklist (M5)

**Code complete:**
- [ ] Armor manifest schema published at `spec/armor-manifest.schema.json`
- [ ] Broker handles all profile types without panicking
- [ ] CLI `run`, `validate`, `audit`, `init` all working
- [ ] Python SDK installable via `pip install mcparmor`
- [ ] Node SDK installable via `npm install mcparmor`
- [ ] Arqitect ships with MCP Armor as its tool execution layer
- [ ] All adversarial tests pass (violations blocked, secrets redacted)

**Documentation:**
- [ ] `README.md` with 5-minute quickstart
- [ ] `docs/manifest-spec.md` — full schema reference
- [ ] `docs/integrations/arqitect.md` — working code example
- [ ] `docs/security-model.md` — honest explanation of what it does and doesn't protect

**Community profiles:**
- [ ] Armor profiles for top 10 community MCP tools in `profiles/community/`
- [ ] At minimum: github, filesystem, gmail, slack, notion, playwright

**Pre-launch:**
- [ ] HN post draft written (Show HN format)
- [ ] ClawHavoc post-mortem article drafted
- [ ] GitHub repo README has demo GIF or terminal recording
- [ ] MIT license file present
- [ ] CONTRIBUTING.md written

---

## Critical Path

```
M0 Spec           ──────────────────────────────────────────────► publish schema
                  │
M1 Broker         └── depends on M0 ──────────────────────────► broker binary
                                     │
M2 CLI            └── depends on M1 ──────────────────────────► mcparmor binary
                                     │
                  ┌──────────────────┤
M3 Python SDK     │  depends on M2   ──────────────────────────► pip install
M4 Node SDK       │  depends on M2   ──────────────────────────► npm install
                  │  (M3 and M4 parallel)
                  │
M5 Launch         └── depends on M2 + M3 + M4 ─────────────────► HN post
```

**Can be parallelized:**
- M3 Python SDK and M4 Node SDK build in parallel after M2
- Community profiles (`profiles/community/`) can be written any time after M0
- Documentation can be written alongside M1/M2
- `mcparmor init` command (static analysis) can be built independently after M2

**Minimum viable v1 (fastest path to launch):**
M0 → M1 → M2 → M3 only. Skip M4 (Node SDK) for initial launch. Add post-launch.

---

## Key Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Enforcement mechanism | Stdio proxy + Python trampoline | Cross-platform, no OS dependencies, covers full Arqitect tool library |
| Policy location | Manifest per tool | Travels with the tool, community-reviewable, no central proxy needed |
| Broker language | Rust | Single binary, cross-platform, fast, zero runtime deps |
| Python interception | Monkey-patch socket.getaddrinfo | Covers all network libs (requests, httpx, urllib) with one hook |
| Secret scanning | Regex set on all responses | Extends proven Arqitect patterns, runs in Rust for speed |
| Audit log format | JSONL append-only | Queryable, tamper-evident, no DB dependency |
| Node SDK timing | Post-launch (M4) | Python covers Arqitect. Node is secondary for v1. |
| OS primitives | Belt-and-suspenders (v2) | Don't block launch on OS-specific work. Broker works everywhere today. |
| Symlink handling | Always resolve before checking | Prevents symlink escape attacks (link /tmp/safe → /etc/passwd) |
| Profile override | Allowed unless `locked: true` | Fabricated tools lock themselves. Community tools can be relaxed by host. |
