# MCP Armor Manifest Specification — armor.json

The armor manifest is the capability declaration that travels with a tool. Every
enforcement decision the broker makes — at both the protocol layer and the OS sandbox
layer — derives from the parsed manifest. This document is the authoritative reference
for every field in `armor.json`.

The canonical JSON Schema is at `spec/v1.0/armor.schema.json` and online at
`https://mcp-armor.com/spec/v1.0/armor.schema.json`.

---

## Manifest anatomy

A minimal valid manifest:

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "strict"
}
```

A fully-specified manifest:

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "min_spec": "1.0",
  "profile": "sandboxed",
  "locked": false,
  "timeout_ms": 30000,
  "filesystem": {
    "read": ["/tmp/mcparmor/*"],
    "write": ["/tmp/mcparmor/*"]
  },
  "network": {
    "allow": ["api.github.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false,
  "env": {
    "allow": ["GITHUB_TOKEN", "PATH"]
  },
  "output": {
    "scan_secrets": true,
    "max_size_kb": 4096
  },
  "audit": {
    "enabled": true,
    "retention_days": 30,
    "max_size_mb": 100,
    "redact_params": false
  }
}
```

---

## Top-level fields

### `$schema`

| Property | Value |
|---|---|
| Type | `string` |
| Required | Yes |
| Constant | `"https://mcp-armor.com/spec/v1.0/armor.schema.json"` |

Identifies the spec version this manifest targets. Must equal the canonical
MCP Armor v1.0 schema URI exactly. Validators and brokers use this to select the
correct schema for validation.

```json
"$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json"
```

### `version`

| Property | Value |
|---|---|
| Type | `string` |
| Required | Yes |
| Pattern | `^\d+\.\d+$` |

The version of this `armor.json` manifest in `MAJOR.MINOR` format. Increment this
when you modify the manifest file itself, not when the tool's code changes.

```json
"version": "1.0"
```

### `min_spec`

| Property | Value |
|---|---|
| Type | `string` |
| Required | No |
| Pattern | `^\d+\.\d+$` |

Minimum MCP Armor broker spec version required to correctly interpret and enforce
this profile. Brokers older than this value must refuse to load the manifest and
return a `manifest_invalid` error rather than silently applying partial enforcement.

Use this when your manifest uses fields introduced in a spec version newer than 1.0.
Omit it for manifests that use only baseline 1.0 fields.

```json
"min_spec": "1.0"
```

### `profile`

| Property | Value |
|---|---|
| Type | `string` |
| Required | Yes |
| Enum | `"strict"`, `"sandboxed"`, `"network"`, `"system"`, `"browser"` |

The named capability profile applied to this tool. Determines the baseline security
posture before per-field overrides are applied. See the Profile Presets table below
for a full description of each profile.

```json
"profile": "sandboxed"
```

### `locked`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `false` |

When `true`, the broker ignores any `--profile` flag passed at invocation time and
treats this manifest's profile declaration as authoritative. This is a cooperative
lock — it signals to compliant runtimes that the tool author's profile must not be
overridden, but it does not prevent a privileged operator from bypassing MCP Armor
entirely.

Use `locked: true` on profiles you distribute with a tool and do not want automated
wrapping tools (like `mcparmor wrap --profile sandboxed`) to override.

```json
"locked": true
```

### `timeout_ms`

| Property | Value |
|---|---|
| Type | `integer` |
| Required | No |
| Default | None (no timeout) |
| Minimum | `100` |
| Maximum | `300000` (5 minutes) |

Maximum wall-clock time in milliseconds allowed for a single tool call. The broker
sends `SIGTERM` to the tool process group when this deadline is exceeded, waits 2
seconds, then sends `SIGKILL`. The broker returns a `timeout` JSON-RPC error
(`-32003`) to the caller.

If omitted, no per-call timeout is enforced. A system-level timeout may still apply
from the MCP host or OS.

```json
"timeout_ms": 30000
```

---

## `filesystem`

Declares the filesystem access policy for this tool. Omitting this object entirely
denies all filesystem access at Layer 1 (param inspection). On supported platforms,
Layer 2 (OS sandbox) also enforces the declared paths at the syscall level.

```json
"filesystem": {
  "read": ["/tmp/mcparmor/*"],
  "write": ["/tmp/mcparmor/*"]
}
```

### `filesystem.read`

| Property | Value |
|---|---|
| Type | `array` of `string` |
| Required | No |
| Default | `[]` (no read access) |

Glob patterns for filesystem paths the tool is allowed to read. Any read attempt
against a path not matched by at least one pattern is blocked at Layer 1 before the
tool receives the call. On Linux 5.13+ and macOS, Layer 2 enforces the same
restriction at the kernel level regardless of how the tool attempts the read.

Patterns follow standard glob syntax: `*` matches any sequence of characters within
a path component, `**` is not supported in v1.

```json
"read": [
  "/tmp/mcparmor/*",
  "/home/user/documents/*"
]
```

### `filesystem.write`

| Property | Value |
|---|---|
| Type | `array` of `string` |
| Required | No |
| Default | `[]` (no write access) |

Glob patterns for filesystem paths the tool is allowed to write or create. Any write
attempt against a path not matched by at least one pattern is blocked at Layer 1 and,
on supported platforms, at the kernel level.

```json
"write": [
  "/tmp/mcparmor/*"
]
```

---

## `network`

Declares the network egress policy for this tool. Omitting this object entirely
denies all outbound network access. Rules are evaluated in priority order:
`deny_metadata` and `deny_local` are checked first, then the `allow` list.

```json
"network": {
  "allow": ["api.github.com:443", "*.googleapis.com:443"],
  "deny_local": true,
  "deny_metadata": true
}
```

### `network.allow`

| Property | Value |
|---|---|
| Type | `array` of `string` |
| Required | No |
| Default | `[]` (no outbound connections) |
| Item pattern | `^(\*|\*\.[a-zA-Z0-9.-]+|[a-zA-Z0-9.-]+):(\*|[0-9]{1,5})$` |

Explicit list of `host:port` combinations the tool may connect to. Items are
evaluated as an allowlist — a connection is permitted if it matches at least one
entry and is not blocked by `deny_local` or `deny_metadata`.

#### Network allow format

Each entry must follow the format `<host>:<port>`:

| Pattern | Description | Example use case |
|---|---|---|
| `api.github.com:443` | Exact hostname, exact port | GitHub REST API |
| `*.googleapis.com:443` | Subdomain wildcard, exact port | Any Google API |
| `*:443` | Any hostname, exact port | SSL certificate checker |
| `localhost:9222` | Exact hostname, exact port | Chrome DevTools Protocol |
| `localhost:*` | Exact hostname, any port | Local dev server |
| `api.github.com:*` | Exact hostname, any port | GitHub on any port |

**What is NOT valid:**

- `*:*` — wildcard host and wildcard port. Declares no capability boundary.
  `mcparmor validate` rejects this.
- `api.github.com` — port is required. Every entry must specify a port.
- `8000-9000` — port ranges are not supported in v1.

**Platform enforcement notes:**

On macOS, `allow` entries are translated into Seatbelt `(allow network-outbound ...)`
clauses. Hostname matching is enforced at the kernel level — the tool cannot connect
to an undeclared host regardless of how the connection is initiated.

On Linux, `allow` entries are enforced by Layer 1 param inspection only for hostname
matching. Landlock TCP (kernel 6.7+) enforces port-level restrictions as an
independent layer, but cannot enforce by hostname. A compiled binary making a direct
`connect()` syscall to an undeclared hostname on a declared port is not blocked at the
kernel level on any Linux version. See `docs/security-model.md` for the full
per-platform enforcement table.

### `network.deny_local`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `true` |

When `true`, the broker blocks all outbound connections to loopback addresses
(`127.0.0.0/8` and `::1`). This prevents tools from accessing services running on
the local machine without explicit authorization.

Set to `false` only when the tool must reach a local service — the most common case
is browser automation tools that connect to a Chrome DevTools Protocol endpoint on
`localhost:9222`. The `browser` profile sets this to `false` automatically.

```json
"deny_local": false
```

**Validation warning:** Setting `deny_local: false` on any profile other than
`browser` causes `mcparmor validate` to print:
```
warn: deny_local: false grants local network access. Ensure this is intentional.
      Browser automation tools require it for CDP; most other tools should use deny_local: true.
```

### `network.deny_metadata`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `true` |

When `true`, the broker blocks all outbound connections to cloud instance metadata
endpoints (`169.254.0.0/16`). This prevents credential theft via SSRF against AWS
(`169.254.169.254`), GCP, Azure, and DigitalOcean metadata services.

This blocks the entire `/16` CIDR, not just the single well-known address. Point
blocks are trivially bypassed — blocking only `169.254.169.254` while leaving the
rest of the range accessible provides no meaningful protection.

On macOS this is enforced by the Seatbelt OS sandbox at the kernel level. On Linux,
it is enforced by Layer 1 param inspection for tools that use JSON-RPC calls.
Compiled binaries on Linux that make direct `connect()` syscalls are not blocked at
the kernel level for this range — see `docs/security-model.md`.

There is no valid reason to set `deny_metadata: false` outside of specialized
infrastructure tooling. Community profiles that set `deny_metadata: false` are
rejected during review.

---

## `spawn`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `false` |

Whether the tool is permitted to spawn child processes. When `false`, any attempt
to fork or exec a subprocess is blocked by the broker. On macOS, Seatbelt enforces
this at the kernel level via `(deny process-exec)`. On Linux, Seccomp blocks the
`execve` and `execveat` syscalls.

Set to `true` only for tools that explicitly require shell or process execution —
for example, browser automation tools that launch a headless browser instance.

```json
"spawn": true
```

When `spawn: true` is set in a community profile submission, the review requires a
documented justification tracing the requirement to the tool's published behavior.

---

## `env`

Declares the environment variable policy for this tool. By default, all environment
variables are stripped from the tool's execution context at spawn time — the tool
subprocess starts with an empty environment. Only variables named in the `allow`
list are forwarded.

```json
"env": {
  "allow": ["GITHUB_TOKEN", "PATH", "HOME"]
}
```

### `env.allow`

| Property | Value |
|---|---|
| Type | `array` of `string` |
| Required | No |
| Default | `[]` (no env vars forwarded) |

Names of environment variables the broker will forward to the tool at spawn time.
All other variables present in the parent process environment are stripped before
the tool starts. Names are matched exactly (case-sensitive).

**Common variables to consider:**

- `PATH` — required by interpreter-based tools (`python`, `node`, `npx`) to locate
  their runtime and dependencies. `mcparmor validate` warns if the tool command
  starts with `python`, `node`, or `npx` and `PATH` is not in the allow list.
- `HOME` — required by tools that need to locate config directories, pip/npm package
  paths, or credential files.
- Tool-specific tokens — `GITHUB_TOKEN`, `SLACK_BOT_TOKEN`, `ANTHROPIC_API_KEY`, etc.

**Diagnostic behavior:** If the tool subprocess exits with code 127 (command not
found) within 2 seconds of start, the broker logs:
```
[mcparmor] tool exited immediately (code 127) — this often means PATH is not
           available. Add "PATH" to env.allow in armor.json.
```

```json
"allow": ["GITHUB_TOKEN", "PATH", "HOME"]
```

---

## `output`

Controls post-processing applied to tool output before it is returned to the caller.
Omitting this object leaves output scanning disabled and imposes no size limit.

```json
"output": {
  "scan_secrets": true,
  "max_size_kb": 4096
}
```

### `output.scan_secrets`

| Property | Value |
|---|---|
| Type | `boolean` or `"strict"` |
| Required | No |
| Default | `false` (scanning disabled) |

Controls secret detection applied to every tool response before it is forwarded to
the MCP host.

| Value | Behavior |
|---|---|
| `false` | Secret scanning is disabled. Responses pass through unchanged. |
| `true` | Detected secrets are redacted in-place: `[REDACTED:openai_key]`. |
| `"strict"` | If any secret is detected, the entire response is blocked and a `secret_blocked` JSON-RPC error (`-32002`) is returned instead. |

The scanner runs against a compiled Rust regex set covering:

- OpenAI API keys (`sk-...`)
- GitHub personal access tokens (`ghp_...`, `ghs_...`)
- AWS access keys (`AKIA...`)
- PEM private keys (`-----BEGIN ... PRIVATE KEY-----`)
- Database connection strings with embedded credentials
- JWT tokens
- Slack tokens (`xox...`)
- Google API keys (`AIza...`)
- Generic key/secret/password/token patterns

Detection events are always logged to the audit log (pattern matched, not the value),
regardless of whether `scan_secrets` is `true` or `"strict"`.

```json
"scan_secrets": "strict"
```

### `output.max_size_kb`

| Property | Value |
|---|---|
| Type | `integer` |
| Required | No |
| Default | None (no size limit) |
| Minimum | `1` |
| Maximum | `102400` (100 MB) |

Maximum permitted size of the tool's response in kilobytes. Responses exceeding this
limit are truncated before delivery to the caller. The truncation point includes a
marker indicating the response was truncated.

```json
"max_size_kb": 4096
```

---

## `audit`

Configures audit logging for tool invocations. Audit records capture call metadata
(tool name, timestamp, session ID, method, param keys) and are written to
`~/.mcparmor/audit/<YYYY-MM-DD>.jsonl`.

```json
"audit": {
  "enabled": true,
  "retention_days": 30,
  "max_size_mb": 100,
  "redact_params": false
}
```

### `audit.enabled`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `true` |

Whether audit logging is active for this tool. Setting to `false` suppresses all
audit records for this tool's invocations. Global audit configuration in
`~/.mcparmor/config.json` can override this on a per-installation basis.

### `audit.retention_days`

| Property | Value |
|---|---|
| Type | `integer` |
| Required | No |
| Default | Inherits global setting (30 days) |
| Minimum | `1` |
| Maximum | `365` |

Number of days audit records for this tool are retained before automatic deletion.

### `audit.max_size_mb`

| Property | Value |
|---|---|
| Type | `integer` |
| Required | No |
| Default | Inherits global setting (500 MB) |
| Minimum | `1` |
| Maximum | `10240` (10 GB) |

Maximum total size in megabytes of stored audit logs before the oldest records are
rotated out.

### `audit.redact_params`

| Property | Value |
|---|---|
| Type | `boolean` |
| Required | No |
| Default | `false` |

When `true`, parameter values are omitted from audit log entries and only parameter
keys are recorded. Use this to prevent sensitive input values (API keys, personal
data passed as arguments) from appearing in audit storage.

Example audit entry without redaction:
```json
{"event":"invoke","method":"create_issue","params":{"repo":"owner/name","title":"Bug","body":"..."}}
```

Example audit entry with `redact_params: true`:
```json
{"event":"invoke","method":"create_issue","params":{"repo":"[omitted]","title":"[omitted]","body":"[omitted]"}}
```

---

## Provenance fields

These fields are prefixed with `_` and are informational. They do not affect
enforcement. They are used by tooling, registries, and auditors to understand where
a profile came from and who reviewed it.

### `_source`

| Property | Value |
|---|---|
| Type | `string` |
| Required | No |

Provenance marker indicating the origin of this profile. Conventional values:

- `"team-authored"` — written by the MCP Armor core team
- `"community"` — submitted through the community PR process

### `_reviewed_by`

| Property | Value |
|---|---|
| Type | `string` |
| Required | No |

Identity of the person or system that reviewed and approved this profile — a GitHub
username or service account name.

### `_reviewed_at`

| Property | Value |
|---|---|
| Type | `string` |
| Required | No |
| Format | ISO 8601 date or date-time |

Date on which this profile was last reviewed and approved.

---

## Profile presets

The `profile` field selects a named preset that establishes the baseline enforcement
posture. Per-field declarations in the manifest overlay the preset — the preset is
the floor, not the ceiling.

| Profile | Filesystem default | Network default | Spawn default | Intended use case |
|---|---|---|---|---|
| `strict` | None | None | `false` | Untrusted tools, AI-generated tools, maximum restriction |
| `sandboxed` | `/tmp/mcparmor/*` r/w | Declared entries only | `false` | Community tools, the safe general-purpose default |
| `network` | None | Declared entries only | `false` | Pure API tools with no local filesystem needs |
| `system` | Declared paths only | Declared hosts only | `false` | OS/system tools that need explicit broad access |
| `browser` | `/tmp/mcparmor/*` r/w | Declared + `localhost:*` | `false` | Browser automation (Playwright, Puppeteer) needing CDP |

**`browser` profile behavior for `deny_local`:**

The `browser` profile implicitly sets `deny_local: false` to allow Chrome DevTools
Protocol connections on `localhost`. If `deny_local: true` is explicitly set
alongside `profile: "browser"`, `mcparmor validate` warns:
```
warn: browser profile requires deny_local: false for CDP.
      Your explicit deny_local: true will be overridden at runtime.
```

The broker ignores the explicit `true` and applies `false` for `browser` profiles.

---

## Annotated examples for common use cases

### Read-only GitHub API tool

A tool that fetches issues and pull requests from GitHub. No filesystem access, one
outbound host, secret scanning to prevent token leakage.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": ["api.github.com:443", "github.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "env": {
    "allow": ["GITHUB_TOKEN", "PATH"]
  },
  "output": {
    "scan_secrets": true
  },
  "audit": {
    "enabled": true,
    "redact_params": true
  }
}
```

`redact_params: true` here prevents repository names and issue titles (which may
contain internal project names) from appearing in audit logs.

### File processing tool with isolated scratch space

A tool that processes uploaded files and writes results. Constrained to a temporary
directory — no access to user home or system paths.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "sandboxed",
  "filesystem": {
    "read": ["/tmp/mcparmor/*"],
    "write": ["/tmp/mcparmor/*"]
  },
  "spawn": false,
  "output": {
    "scan_secrets": false,
    "max_size_kb": 8192
  },
  "timeout_ms": 60000
}
```

`max_size_kb: 8192` prevents the tool from returning very large file contents that
could exhaust the host's context window.

### Browser automation tool (Playwright)

A tool that launches a headless browser and navigates the web. Requires `spawn: true`
to launch the browser subprocess and `deny_local: false` for the CDP connection.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "browser",
  "timeout_ms": 60000,
  "network": {
    "allow": ["*:443", "*:80"],
    "deny_local": false,
    "deny_metadata": true
  },
  "filesystem": {
    "read": ["/Users/*/.config/playwright/*"]
  },
  "spawn": true,
  "output": {
    "scan_secrets": true,
    "max_size_kb": 8192
  }
}
```

`spawn: true` is required here — Playwright must launch the browser process.
`deny_metadata: true` remains enforced even with broad network access, preventing
SSRF to cloud metadata services via browser navigation.

### Locked strict profile for AI-generated tools

A tool generated by an AI agent that should run with maximum restriction and
cannot have its profile overridden by a wrapping tool.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "strict",
  "locked": true,
  "output": {
    "scan_secrets": "strict"
  },
  "audit": {
    "enabled": true,
    "redact_params": false
  }
}
```

`locked: true` prevents `mcparmor wrap --profile sandboxed` from loosening the
profile. `scan_secrets: "strict"` blocks the entire response if any secret is
detected — appropriate for tools where the origin of output is not fully trusted.

### Slack messaging tool with strict secret scanning

A tool that posts messages and reads channel history. Uses `scan_secrets: "strict"`
because any Slack bot token that leaks in a response must be treated as a hard
failure, not just redacted.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": [
      "slack.com:443",
      "api.slack.com:443",
      "files.slack.com:443"
    ],
    "deny_local": true,
    "deny_metadata": true
  },
  "env": {
    "allow": ["SLACK_BOT_TOKEN"]
  },
  "output": {
    "scan_secrets": "strict"
  },
  "audit": {
    "enabled": true,
    "redact_params": true
  }
}
```

### SQLite database tool with narrow file scope

A tool that reads and writes a specific SQLite database. Restricted to `.db` and
`.sqlite` files in the MCP Armor scratch directory.

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "sandboxed",
  "filesystem": {
    "read": ["/tmp/mcparmor/*.db", "/tmp/mcparmor/*.sqlite"],
    "write": ["/tmp/mcparmor/*.db", "/tmp/mcparmor/*.sqlite"]
  },
  "spawn": false,
  "output": {
    "scan_secrets": false
  }
}
```

`scan_secrets: false` is appropriate here because the tool is reading structured
database content, not secrets, and scanning would add latency without benefit.

---

## Validation

Run `mcparmor validate` against any manifest before use:

```bash
mcparmor validate armor.json
```

The validator checks:

- JSON Schema conformance against `spec/v1.0/armor.schema.json`
- `network.allow` patterns for format validity
- `*:*` wildcard rejection
- `deny_local: false` without `profile: "browser"` (warning)
- `deny_metadata: false` (error — not permitted in v1 community profiles)
- Missing `PATH` in `env.allow` for interpreter-based tools (warning)
- `spawn: true` justification note (warning — not an error, but noted in review)

---

## Spec versioning

Schema URLs follow `https://mcp-armor.com/spec/v{MAJOR}.{MINOR}/armor.schema.json`.

- **Patch changes** — clarified descriptions, new secret scanner patterns: no schema
  version bump.
- **Minor changes** — new optional fields: bump minor (`1.0` → `1.1`). Old manifests
  remain valid.
- **Breaking changes** — removed fields, changed enums, new required fields: bump
  major (`1.0` → `2.0`). The broker supports both major versions during a transition
  window.

The `version` field in the manifest declares which schema version it was written
against. The broker validates using that version's schema — not the latest.
