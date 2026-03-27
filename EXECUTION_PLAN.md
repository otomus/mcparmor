# MCP Armor — Technical Execution Plan

> Stack: Rust (core/broker/CLI), Python SDK, Node SDK
> Principle: Framework-agnostic, host-agnostic, language-agnostic
> Showcase: Arqitect (reference consumer — not the design target)

---

## Guiding Principle

MCP Armor is neutral infrastructure. It knows nothing about Arqitect, OpenClaw,
NanoClaw, or any specific agent framework. It knows about:

- The MCP protocol (JSON-RPC over stdio)
- Armor manifests (standalone `armor.json` files)
- Subprocess execution

Any MCP tool, written in any language, run by any host, gets the same protection.
Arqitect is one showcase among many — not the blueprint.

---

## Repository Structure

```
mcparmor/
├── Cargo.toml                          ← workspace manifest
├── crates/
│   ├── mcparmor-core/                  ← types, manifest parsing, secret scanner
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manifest.rs             ← ArmorManifest, parse + validate
│   │       ├── policy.rs               ← capability enforcement decisions
│   │       ├── scanner.rs              ← secret/PII output scanning
│   │       └── audit.rs               ← AuditLog, AuditEvent types
│   ├── mcparmor-broker/                ← stdio proxy process
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── proxy.rs                ← JSON-RPC stdio proxy loop
│   │       └── interceptor.rs          ← param inspection, path/host validation
│   └── mcparmor-cli/                   ← the `mcparmor` binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
├── spec/
│   └── armor.schema.json               ← THE canonical schema (standalone file)
├── sdks/
│   ├── python/                         ← pip install mcparmor
│   └── node/                           ← npm install mcparmor
├── showcases/
│   ├── arqitect/                       ← how Arqitect integrates MCP Armor
│   ├── openclaw/                       ← how OpenClaw could integrate
│   └── langchain/                      ← how LangChain tools could use it
├── profiles/
│   └── community/                      ← armor profiles for popular MCP tools
│       ├── github.armor.json
│       ├── filesystem.armor.json
│       ├── playwright.armor.json
│       └── ...
├── docs/
│   ├── getting-started.md
│   ├── manifest-spec.md
│   ├── security-model.md
│   └── integrations.md
└── tests/
    ├── adversarial/                    ← tools that try to escape
    └── fixtures/
        └── tools/                      ← language-agnostic test tools
            ├── python/
            ├── node/
            ├── go/
            └── rust/
```

---

## M0 — Armor Manifest Spec

### Decision: Standalone `armor.json` — not embedded in any framework's manifest

MCP Armor has no opinion about how a tool is packaged. The armor manifest is a
separate file. Tool authors ship it alongside their tool however they package it.

```
my-tool/
  tool.py          ← the tool (any language, any structure)
  armor.json       ← the armor manifest (MCP Armor's only concern)
```

Frameworks that want to embed the armor block in their own manifest (e.g. Arqitect's
`tool.json`) can do so — MCP Armor accepts either a path to `armor.json` or an
inline `armor` block extracted from any JSON file. That's the framework's choice,
not ours.

### Schema Design

```json
{
  "$schema": "https://mcparmor.io/spec/1.0/armor.schema.json",
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
  },

  "locked": false
}
```

### Profile Presets

| Profile | filesystem | network | spawn | Use case |
|---|---|---|---|---|
| `strict` | none | none | false | Untrusted / AI-generated tools |
| `sandboxed` | `/tmp/mcparmor/*` r/w | declared only | false | Community tools (default) |
| `network` | none | declared only | false | Pure API tools |
| `system` | declared paths | declared hosts | false | System/OS tools |
| `browser` | `/tmp/mcparmor/*` r/w | `*:443` | false | Browser automation |

`locked: true` means no MCP host can relax the profile at runtime. Mandatory for
AI-generated tools.

### Real Examples (language-agnostic)

**A Go SSL checker tool:**
```json
{
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": ["*:443"],
    "deny_local": true
  }
}
```

**A Node.js GitHub MCP server:**
```json
{
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": ["api.github.com:443", "github.com:443"],
    "deny_local": true
  },
  "env": {
    "allow": ["GITHUB_TOKEN"],
    "deny_system": true
  }
}
```

**A Python browser automation tool:**
```json
{
  "version": "1.0",
  "profile": "browser",
  "filesystem": {
    "read": ["/tmp/mcparmor/*"],
    "write": ["/tmp/mcparmor/*"]
  },
  "network": {
    "allow": ["localhost:*"],
    "deny_metadata": true
  }
}
```

**An AI-generated tool (locked strict):**
```json
{
  "version": "1.0",
  "profile": "strict",
  "locked": true
}
```

### JSON Schema (`armor.schema.json`)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://mcparmor.io/spec/1.0/armor.schema.json",
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
        "timeout_ms":    { "type": "integer", "default": 30000 },
        "max_memory_mb": { "type": "integer", "default": 256 }
      }
    }
  }
}
```

---

## M1 — Broker Architecture

### Core Decision: Stdio Proxy Only (language-agnostic by default)

The broker is a **stdio proxy**. It wraps any subprocess command. It knows nothing
about Python, Node, Go, or Rust. It only knows JSON-RPC over stdio.

```
Any MCP Host
   ↕ (stdio JSON-RPC)
[mcparmor broker]
   reads armor.json
   spawns: <any command>
   proxies JSON-RPC both directions
   validates: params (paths, URLs in param values)
   scans: responses for secrets
   enforces: timeout, memory limit
   logs: all events to audit log
   ↕ (stdio JSON-RPC)
<any MCP tool subprocess>
  python tool.py
  node tool.js
  ./tool (Go binary)
  ./tool (Rust binary)
  npx @modelcontextprotocol/server-github
```

No language-specific trampolines in the core. Language-specific interceptors
are optional SDK extensions — not broker concerns.

### Process Lifecycle

```
1. MCP Host spawns:
   mcparmor run --armor armor.json -- <any command>

2. Broker:
   a. Reads + validates armor.json
   b. Spawns tool subprocess with restricted env (only declared env vars)
   c. Enters proxy loop

3. Proxy loop per JSON-RPC message:
   a. Read line from MCP Host stdin
   b. Parse JSON-RPC request
   c. Inspect param values for path/URL references → validate against manifest
   d. Forward approved request to tool stdin
   e. Read response from tool stdout
   f. Scan response for secrets → redact or block
   g. Log full event to audit log
   h. Forward clean response to MCP Host stdout

4. On timeout: SIGTERM tool, wait 2s, SIGKILL, return JSON-RPC error
5. On capability violation: log, return JSON-RPC error (do not kill — tool can continue)
6. On secret detected: log + redact, forward sanitized response
```

### Param Inspection

The broker inspects param values in JSON-RPC requests for path and URL references
before forwarding. This catches cases where the tool receives a path from the host
and uses it for filesystem access:

```json
{"method": "read_file", "params": {"path": "/etc/passwd"}}
```

The broker validates `path` against `filesystem.read` before the tool ever sees it.

Pattern detection for param values:
- Absolute paths: `/`, `C:\`, `~`
- Relative path traversal: `../`
- URLs: `http://`, `https://`, `ftp://`
- Local addresses: `localhost`, `127.0.0.1`, `::1`, `169.254.169.254` (metadata)

### Secret Scanner

Compiled Rust regex set. Runs on every response before forwarding:

```rust
pub const SECRET_PATTERNS: &[(&str, &str)] = &[
    (r"sk-[A-Za-z0-9]{20,}", "openai_key"),
    (r"ghp_[A-Za-z0-9]{36,}", "github_pat"),
    (r"ghs_[A-Za-z0-9]{36,}", "github_app_token"),
    (r"AKIA[A-Z0-9]{16}", "aws_access_key"),
    (r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----", "private_key"),
    (r"(?:mongodb|postgres|mysql|redis)://[^\s\"']+:[^\s\"']+@", "db_connection"),
    (r#"(?i)(?:api.?key|secret|password|token)\s*[=:]\s*["']?[A-Za-z0-9\-._~+/]{16,}"#, "generic_secret"),
    (r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+", "jwt_token"),
    (r"xox[baprs]-[A-Za-z0-9\-]{10,}", "slack_token"),
    (r"AIza[A-Za-z0-9\-_]{35}", "google_api_key"),
];
```

On detection:
- Default: redact in-place → `[REDACTED:openai_key]`
- If `scan_secrets: "strict"`: return JSON-RPC error, block response entirely
- Always: log detection event with tool name, call_id, pattern matched (not the value)

### Audit Log (JSONL, append-only)

```jsonl
{"ts":"2026-03-27T10:00:00.123Z","session":"abc","tool":"github_server","event":"invoke","call_id":"u1","method":"list_issues","params":{"repo":"owner/name"}}
{"ts":"2026-03-27T10:00:00.200Z","session":"abc","tool":"github_server","event":"network_check","call_id":"u1","host":"api.github.com","port":443,"allowed":true}
{"ts":"2026-03-27T10:00:01.800Z","session":"abc","tool":"github_server","event":"response","call_id":"u1","secret_scan":"clean","latency_ms":1600,"bytes":4200}
{"ts":"2026-03-27T10:00:05.000Z","session":"abc","tool":"any_tool","event":"param_violation","call_id":"u2","param":"path","value":"/etc/passwd","action":"blocked"}
{"ts":"2026-03-27T10:00:08.000Z","session":"abc","tool":"any_tool","event":"secret_detected","call_id":"u3","pattern":"openai_key","action":"redacted"}
```

Written to: `~/.mcparmor/audit/<YYYY-MM-DD>.jsonl`

---

## M2 — CLI Design

### Commands

```
mcparmor run       Run any MCP tool subprocess under armor
mcparmor validate  Validate an armor manifest file
mcparmor audit     Query the audit log
mcparmor init      Generate an armor.json for a tool
mcparmor profile   List and describe built-in profiles
```

### `mcparmor run`

```
USAGE:
  mcparmor run [OPTIONS] -- <COMMAND> [ARGS...]

OPTIONS:
  -a, --armor <FILE>         Path to armor.json [default: ./armor.json]
  --profile <PROFILE>        Override profile (blocked if armor has locked: true)
  --audit-log <FILE>         Audit log path [default: ~/.mcparmor/audit/today.jsonl]
  --no-audit                 Disable audit logging
  --strict                   Any violation = fatal (exit 2)
  -v, --verbose              Print capability decisions to stderr

EXAMPLES:
  # Python tool
  mcparmor run -- python tool.py

  # Node MCP server
  mcparmor run --armor ./armor.json -- npx -y @modelcontextprotocol/server-github

  # Go binary
  mcparmor run --armor ./armor.json -- ./my-tool

  # Rust binary with explicit profile override
  mcparmor run --armor ./armor.json --profile strict -- ./my-tool

EXIT CODES:
  0   Success
  1   Tool returned a JSON-RPC error
  2   Capability violation (blocked by armor)
  3   Armor manifest invalid or not found
  4   Timeout
  5   Tool subprocess crashed
```

### `mcparmor validate`

```
USAGE:
  mcparmor validate [--armor <FILE>]

OUTPUT (success):
  ✓ armor.json — valid
    profile:    sandboxed
    filesystem: read [/tmp/mcparmor/*]  write [/tmp/mcparmor/*]
    network:    allow [api.github.com:443]  deny_local: true
    spawn:      false
    output:     secret scan enabled
    locked:     false

OUTPUT (failure):
  ✗ armor.json — invalid
    line 8: "profile" must be one of: strict, sandboxed, network, system, browser
    line 15: "network.allow" items must be in format "host:port" or "host:*"
```

### `mcparmor audit`

```
USAGE:
  mcparmor audit [OPTIONS]

OPTIONS:
  --tool <NAME>        Filter by tool/command name
  --event <TYPE>       invoke | violation | secret_detected | response
  --since <DATETIME>   ISO8601 or relative (1h, 24h, 7d)
  --format table|json  Output format [default: table]

TABLE OUTPUT:
  TIME                  TOOL             EVENT             DETAIL
  2026-03-27 10:00:00   github_server    invoke            list_issues
  2026-03-27 10:00:01   github_server    response          clean 4.2KB 1600ms
  2026-03-27 10:00:05   unknown_tool     param_violation   /etc/passwd BLOCKED
  2026-03-27 10:00:08   unknown_tool     secret_detected   openai_key REDACTED
```

### `mcparmor init`

```
USAGE:
  mcparmor init [--dir <DIR>] [--profile <PROFILE>]

Generates a minimal armor.json in the given directory.
Does NOT analyze source code — keeps tool author in control.
Interactive mode confirms each capability.

  MCP Armor — armor.json generator

  Profile [sandboxed]:
  Filesystem read paths (comma-separated, blank for none): /tmp/mcparmor/*
  Filesystem write paths (comma-separated, blank for none): /tmp/mcparmor/*
  Network allow (host:port, comma-separated, blank for none): api.github.com:443
  Allow spawn? [n]:
  Env vars allowed (comma-separated, blank for none): GITHUB_TOKEN
  Lock profile? [n]:

  Writing armor.json... done.
  Validate: mcparmor validate
```

---

## M3 — Python SDK

### Design Principle

The Python SDK wraps the `mcparmor` CLI binary. It does not re-implement the
broker in Python. This guarantees behavior parity with the CLI and every other SDK.

### API Surface

```python
from mcparmor import ArmoredProcess, ArmorManifest

# --- Wraps any subprocess command ---
# Works with Python, Node, Go, Rust — anything

proc = ArmoredProcess(
    command=["python", "tool.py"],
    armor="./armor.json",        # path OR inline dict
    cwd="/path/to/tool"
)

# Single call (spawns, calls, kills)
result = proc.invoke({"method": "run", "params": {"domain": "example.com"}})

# Persistent (spawn once, call many times)
with ArmoredProcess(["npx", "-y", "@mcp/github"], armor="./armor.json") as proc:
    r1 = proc.invoke({"method": "list_repos", "params": {}})
    r2 = proc.invoke({"method": "get_issue",  "params": {"number": 42}})

# --- Manifest inspection (no broker needed) ---
manifest = ArmorManifest.load("./armor.json")
manifest.profile                              # "sandboxed"
manifest.allows_network("api.github.com", 443)  # True
manifest.allows_path_read("/etc/passwd")        # False
manifest.is_locked()                          # False

# --- Low-level: get an armored Popen-compatible object ---
from mcparmor import armor_popen

proc = armor_popen(
    ["python", "tool.py"],
    armor="./armor.json"
)
# proc.stdin / proc.stdout — standard JSON-RPC pipe
# Drop-in for any framework that manages its own subprocess lifecycle
```

### `armor_popen` — The Integration Primitive

This is the key integration point for any Python framework. It returns a standard
`subprocess.Popen`-compatible object. Frameworks don't need to change their
subprocess management — just swap the spawn call.

```python
# Before (any Python agent framework)
proc = subprocess.Popen(["python", "tool.py"], stdin=PIPE, stdout=PIPE)

# After — one line change
proc = armor_popen(["python", "tool.py"], armor="armor.json")

# Same interface — proc.stdin, proc.stdout, proc.wait(), proc.kill()
```

### Packaging

```
pip install mcparmor
```

Bundles the `mcparmor` Rust binary for the current platform as package data.
Supports: linux-x86_64, linux-aarch64, darwin-x86_64, darwin-aarch64, windows-x86_64.

---

## M4 — Node SDK

### Design Principle

Same as Python SDK — wraps the CLI binary, does not reimplement the broker.

### API Surface

```typescript
import { ArmoredProcess, ArmorManifest, armorSpawn } from 'mcparmor';

// Wraps any command — Python, Node, Go, Rust, npx packages
const proc = new ArmoredProcess({
  command: ['npx', '-y', '@modelcontextprotocol/server-github'],
  armor: './armor.json'
});

const result = await proc.invoke({ method: 'list_repos', params: {} });
await proc.close();

// Persistent subprocess
const proc = await ArmoredProcess.spawn({
  command: ['node', 'tool.js'],
  armor: './armor.json'
});
const r1 = await proc.invoke(params1);
const r2 = await proc.invoke(params2);
await proc.close();

// Manifest inspection
const manifest = ArmorManifest.load('./armor.json');
manifest.allowsNetwork('api.github.com', 443);  // true
manifest.isLocked();                             // false

// Low-level ChildProcess-compatible — drop-in for any Node framework
import { armorSpawn } from 'mcparmor';
const child = armorSpawn(['node', 'tool.js'], { armor: './armor.json' });
// child.stdin, child.stdout — standard Node ChildProcess interface
```

---

## Cross-Platform Strategy

### Layer 1 — Broker (always active, all platforms)
Protocol-level enforcement via stdio proxy. Works on macOS, Linux, Windows,
anywhere the binary runs. This is v1. Sufficient for launch.

### Layer 2 — OS Primitives (v2, opt-in)

Generated from `armor.json` automatically. Applied to the tool subprocess
at spawn time. Belt-and-suspenders over Layer 1.

**Linux — Seccomp + Landlock:**
```
armor.json filesystem.read → Landlock path rules
armor.json filesystem.write → Landlock path rules
armor.json spawn: false → Seccomp block execve
```

**macOS — Seatbelt:**
```
armor.json → generated sandbox-exec profile string
applied via: sandbox_init(profile, 0, &err)
```

**Windows — AppContainer (v3, lowest priority):**
```
armor.json → AppContainer capability set + job object limits
```

OS primitives are enabled via flag: `mcparmor run --os-sandbox -- ...`
Not default in v1 — don't gate launch on OS-specific implementation.

---

## Testing Strategy

### Principle: Test behavior, not implementation

Tests use the CLI directly — `mcparmor run -- <tool>`. This tests the full
stack regardless of language. Language-specific SDK tests use `armor_popen` /
`armorSpawn` as thin wrappers and verify they produce identical behavior.

### Adversarial Test Tools (one per language, same behavior expected)

Each adversarial tool exists in Python, Node, Go:

```
tests/adversarial/
  read_passwd/
    tool.py     ← tries open("/etc/passwd")
    tool.js     ← tries fs.readFileSync("/etc/passwd")
    tool.go     ← tries os.Open("/etc/passwd")
  call_forbidden/
    tool.py     ← tries requests.get("http://evil.com")
    tool.js     ← tries fetch("http://evil.com")
    tool.go     ← tries http.Get("http://evil.com")
  leak_secret/
    tool.py     ← returns hardcoded "sk-abc123..." in response
    tool.js     ← same
    tool.go     ← same
  spawn_child/
    tool.py     ← tries subprocess.run(["ls"])
    tool.js     ← tries child_process.spawn("ls")
    tool.go     ← tries exec.Command("ls").Run()
```

Expected behavior for all: capability violation (exit 2) or redacted response.
The broker must behave identically regardless of tool language.

### Fixture Tools (legitimate, should succeed)

```
tests/fixtures/tools/
  echo/         ← returns params unchanged, no I/O
  http_get/     ← fetches declared host, returns response
  file_read/    ← reads from /tmp/mcparmor/, returns content
```

### CI Matrix

| OS | Languages tested | Suite |
|---|---|---|
| ubuntu-latest | Python 3.10/3.12, Node 20/22, Go 1.22 | full |
| macos-latest | Python 3.12, Node 22 | full |
| windows-latest | Python 3.12, Node 22 | broker + CLI only |

### Property-Based Tests

Fuzz manifest parsing with `proptest` — random JSON, assert no panics.
Fuzz param inspection — random param values, assert no path traversal escapes.

---

## Showcases (not docs — working code)

Each showcase is a self-contained working integration in `showcases/`:

### `showcases/arqitect/`
Shows how Arqitect's ToolManager uses `armor_popen` to wrap its mcp_tools.
One file. Demonstrates the `armor` block inside Arqitect's `tool.json` format.

### `showcases/openclaw/`
Shows how an OpenClaw skill runner could use `armorSpawn` to wrap skills.
Demonstrates the standalone `armor.json` file pattern.

### `showcases/langchain/`
Shows how a LangChain tool executor could wrap MCP tools with armor.
Demonstrates the Python SDK `ArmoredProcess` class.

### `showcases/bare_cli/`
Shows the simplest possible usage — just the CLI, no SDK, any language.
The README equivalent of "hello world."

---

## Launch Checklist (M5)

**Code complete:**
- [ ] `armor.schema.json` published and versioned
- [ ] Broker handles all profiles, all param types, no panics on malformed input
- [ ] CLI: `run`, `validate`, `audit`, `init` all working
- [ ] Python SDK: `pip install mcparmor` — `armor_popen` works on macOS + Linux
- [ ] Node SDK: `npm install mcparmor` — `armorSpawn` works on macOS + Linux
- [ ] All adversarial tests pass across Python, Node, Go tools
- [ ] Secret scanner tested against all pattern types

**Documentation:**
- [ ] README: 2-minute quickstart using bare CLI (no SDK)
- [ ] `docs/manifest-spec.md` — full schema reference
- [ ] `docs/security-model.md` — honest model + limitations
- [ ] `docs/integrations.md` — generic integration guide (not framework-specific)
- [ ] `showcases/` — all three working showcases committed

**Community profiles:**
- [ ] `armor.json` profiles for top 10 MCP tools in `profiles/community/`
  Minimum: github, filesystem, gmail, slack, notion, playwright, fetch, git

**Pre-launch:**
- [ ] HN post draft: "Show HN: MCP Armor — capability protection for MCP tools"
- [ ] ClawHavoc post-mortem article drafted
- [ ] Demo terminal recording: `mcparmor run` blocking a path traversal attempt
- [ ] MIT license present
- [ ] CONTRIBUTING.md: how to submit community armor profiles

---

## Critical Path

```
M0 Spec ──────────────────────────────────────────────────► armor.schema.json published
   │
M1 Broker ─── depends on M0 ──────────────────────────────► broker binary working
   │
M2 CLI ─────── depends on M1 ──────────────────────────────► mcparmor binary
   │                │
   │         ┌──────┤
M3 Python ───┤      ├─── parallel after M2 ─────────────────► pip install mcparmor
M4 Node ─────┘      └─── parallel after M2 ─────────────────► npm install mcparmor
   │
M5 Launch ─── depends on M2 + M3 + docs + showcases ──────► HN post
```

**Minimum viable v1 (fastest path to launch):**
M0 → M1 → M2 → M3 → showcases + docs → launch.
Node SDK (M4) can ship one week post-launch.

---

## Key Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Manifest file | Standalone `armor.json` | Framework-agnostic. Any tool, any packaging. |
| Broker mechanism | Stdio proxy only (no trampolines in core) | Language-agnostic. Works with any subprocess. |
| Integration primitive | `armor_popen` / `armorSpawn` | One-line change for any framework. Same interface as stdlib. |
| Param inspection | Validate path/URL values before forwarding | Catches the attack vector before the tool sees it. |
| Secret scanner | Rust regex set on all responses | Fast, cross-language, runs regardless of tool implementation. |
| OS primitives | Optional, v2, behind a flag | Don't block launch. Broker is sufficient for v1. |
| Test language coverage | Python + Node + Go adversarial tools | Proves broker is truly language-agnostic. |
| Showcases location | `showcases/` not `docs/` | Working code, not prose. Frameworks copy-paste from here. |
| Arqitect references | Showcase only, not in core | MCP Armor is neutral infrastructure. Arqitect is one consumer. |
