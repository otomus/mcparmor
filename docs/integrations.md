# MCP Armor Integration Guide

MCP Armor is neutral infrastructure. It knows nothing about any specific agent
framework or MCP host. This guide covers how to integrate MCP Armor into any
environment that launches MCP tool subprocesses.

---

## How `mcparmor run` works

MCP Armor operates as a **stdio proxy**. The broker process sits between the MCP
host and the tool subprocess, forwarding JSON-RPC messages over stdin/stdout while
enforcing the declared capability manifest.

```
MCP Host
  │ (stdio JSON-RPC)
  ▼
mcparmor broker   ← reads armor.json, applies OS sandbox, proxies messages
  │ (stdio JSON-RPC)
  ▼
Tool subprocess   ← Python, Node, Go, Rust, any language
```

The broker:

1. Reads and validates the `armor.json` manifest.
2. Spawns the tool subprocess with a restricted environment (only declared env vars).
3. Applies the OS-level sandbox to the tool subprocess before the first message
   is forwarded (Layer 2 — Seatbelt on macOS, Landlock + Seccomp on Linux).
4. Enters a proxy loop: reads requests from the host, validates params against the
   manifest, forwards approved requests to the tool, scans responses for secrets,
   forwards clean responses to the host.

From the MCP host's perspective, it is talking to the broker, not the tool directly.
The broker's stdin/stdout are the tool's stdin/stdout from the host's point of view.
No changes to the MCP protocol are required on the host side.

### CLI signature

```
mcparmor run [OPTIONS] -- <command> [args...]
```

| Option | Description |
|---|---|
| `--armor <path>` | Path to the `armor.json` manifest. If omitted, the broker searches the tool's directory for `armor.json`. If not found, applies `strict` profile by default. |
| `--profile <name>` | Override the base profile declared in `armor.json`. Ignored if the manifest sets `locked: true`. |
| `--no-os-sandbox` | Disable Layer 2 OS sandbox. Layer 1 (protocol enforcement) remains active. Use only when the OS sandbox is incompatible with the runtime environment. |

The `--` separator separates broker flags from the tool command and its arguments.

---

## Integration patterns

### Pattern 1: Wrap an existing host config with `mcparmor wrap`

The fastest path for end users. One command wraps all tools in a supported MCP host
configuration.

```bash
# Claude Desktop
mcparmor wrap --host claude-desktop

# Cursor (project scope)
mcparmor wrap --host cursor --scope project

# VS Code
mcparmor wrap --host vscode

# Windsurf
mcparmor wrap --host windsurf

# Preview changes without modifying anything
mcparmor wrap --host claude-desktop --dry-run
```

`mcparmor wrap` modifies the host's configuration file in place, prepending
`mcparmor run` to each tool's command. It creates a `.bak` backup before modifying.

**Before (claude_desktop_config.json):**
```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_TOKEN": "ghp_..." }
    }
  }
}
```

**After:**
```json
{
  "mcpServers": {
    "github": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/home/user/.mcparmor/discovered/github.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-github"
      ],
      "env": { "GITHUB_TOKEN": "ghp_..." }
    }
  }
}
```

All paths written by `mcparmor wrap` are absolute. The `~` shorthand is not used
because MCP hosts spawn subprocesses directly without a shell, so `~` is not
expanded.

HTTP/SSE transport tools (those with a `"url"` key instead of `"command"`) are
skipped automatically with a warning — the broker can only wrap local subprocess
tools.

### Pattern 2: Direct `mcparmor run` in any config

Any MCP host that accepts a `command` + `args` tool definition can be integrated
by replacing the tool command with `mcparmor run`.

Generic form:
```json
{
  "command": "mcparmor",
  "args": [
    "run",
    "--armor", "/absolute/path/to/armor.json",
    "--",
    "original-tool-command", "arg1", "arg2"
  ]
}
```

Example for a custom Python tool:
```json
{
  "command": "mcparmor",
  "args": [
    "run",
    "--armor", "/home/user/my-tool/armor.json",
    "--",
    "python3", "/home/user/my-tool/tool.py"
  ]
}
```

### Pattern 3: Python SDK

For MCP host implementations written in Python, the `mcparmor` Python package
provides `armor_popen` — a drop-in replacement for `subprocess.Popen` that wraps
the tool command with the broker.

### Pattern 4: Node.js SDK

For MCP host implementations written in TypeScript/JavaScript, the `mcparmor` npm
package provides `armorSpawn` — a drop-in replacement for `child_process.spawn`.

---

## Python SDK

Install from PyPI:

```bash
pip install otomus-mcp-armor
```

### Basic usage

```python
from mcparmor import armor_popen

proc = armor_popen(
    ["python3", "/path/to/tool.py"],
    armor="/path/to/armor.json",
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

`armor_popen` returns a standard `subprocess.Popen` object. The process's
stdin/stdout are connected to the broker's stdio, which proxies to the tool.
Use it exactly as you would a `subprocess.Popen` result.

### Function signature

```python
def armor_popen(
    command: list[str],
    *,
    armor: str | Path | None = None,
    profile: str | None = None,
    no_os_sandbox: bool = False,
    **popen_kwargs: Any,
) -> subprocess.Popen:
```

| Parameter | Type | Description |
|---|---|---|
| `command` | `list[str]` | The tool command and arguments. |
| `armor` | `str \| Path \| None` | Path to `armor.json`. If `None`, the broker searches the tool's directory. |
| `profile` | `str \| None` | Profile override. Ignored if the manifest sets `locked: true`. |
| `no_os_sandbox` | `bool` | Disable Layer 2 OS sandbox. Default `False`. |
| `**popen_kwargs` | | Forwarded to `subprocess.Popen` (e.g. `env`, `cwd`). |

Raises `ArmorPopenError` if the `mcparmor` binary cannot be found or the broker
process fails to start. Raises `ValueError` if `command` is empty.

### Integration with an MCP host framework

```python
import subprocess
from mcparmor import armor_popen, ArmorPopenError

def spawn_tool(tool_config: dict) -> subprocess.Popen:
    """Spawn a tool subprocess, applying MCP Armor if available."""
    command = [tool_config["command"]] + tool_config.get("args", [])
    armor_path = tool_config.get("armor")

    try:
        return armor_popen(
            command,
            armor=armor_path,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except ArmorPopenError as exc:
        # mcparmor binary not installed — fall back to unprotected spawn
        # Log a warning here in a real implementation
        return subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
```

### Environment variable passthrough

The broker strips all environment variables from the tool subprocess by default.
Only variables declared in `armor.json` under `env.allow` are forwarded. Do not
pass an `env` kwarg to `armor_popen` to override the tool's environment — the
broker manages this from the manifest. The `env` kwarg is forwarded to the
broker process itself, not the tool.

### Profile override at runtime

```python
# Force strict profile regardless of what armor.json declares (unless locked)
proc = armor_popen(
    ["node", "/path/to/tool.js"],
    armor="/path/to/armor.json",
    profile="strict",
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

### Disabling the OS sandbox

```python
# Layer 1 (protocol enforcement) only — use when OS sandbox is incompatible
proc = armor_popen(
    ["python3", "/path/to/tool.py"],
    armor="/path/to/armor.json",
    no_os_sandbox=True,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

---

## Node.js SDK

Install from npm:

```bash
npm install mcparmor
```

### Basic usage

```typescript
import { armorSpawn } from 'mcparmor';

const proc = armorSpawn(
  ['node', '/path/to/tool.js'],
  {
    armor: '/path/to/armor.json',
    stdio: ['pipe', 'pipe', 'pipe'],
  }
);
```

`armorSpawn` returns a standard `ChildProcess` object from `node:child_process`.
Use it exactly as you would a `child_process.spawn` result.

### Function signature

```typescript
function armorSpawn(
  command: readonly string[],
  options: ArmorSpawnOptions = {},
): ChildProcess
```

```typescript
interface ArmorSpawnOptions extends SpawnOptions {
  /** Path to the armor.json capability manifest. */
  readonly armor?: string;
  /** Override the base profile declared in armor.json. */
  readonly profile?: string;
  /** Disable OS-level sandbox (skips Layer 2 enforcement). */
  readonly noOsSandbox?: boolean;
}
```

`ArmorSpawnOptions` extends the standard `SpawnOptions` from `node:child_process`.
All standard options (`stdio`, `cwd`, `env`, etc.) are forwarded to the broker
spawn call.

Throws `TypeError` if `command` is not a non-empty array of strings. Throws
`BinaryNotFoundError` if the `mcparmor` binary cannot be located.

### Integration with an MCP host framework

```typescript
import { armorSpawn, BinaryNotFoundError } from 'mcparmor';
import { spawn, type ChildProcess } from 'node:child_process';

interface ToolConfig {
  command: string;
  args?: string[];
  armor?: string;
}

function spawnTool(config: ToolConfig): ChildProcess {
  const command = [config.command, ...(config.args ?? [])];

  try {
    return armorSpawn(command, {
      armor: config.armor,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
  } catch (err) {
    if (err instanceof BinaryNotFoundError) {
      // mcparmor binary not installed — fall back to unprotected spawn
      // Emit a warning in a real implementation
      return spawn(config.command, config.args ?? [], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    }
    throw err;
  }
}
```

### Profile override and sandbox control

```typescript
// Override profile
const proc = armorSpawn(['node', 'tool.js'], {
  armor: './armor.json',
  profile: 'strict',
  stdio: ['pipe', 'pipe', 'pipe'],
});

// Disable OS sandbox (Layer 1 only)
const proc = armorSpawn(['node', 'tool.js'], {
  armor: './armor.json',
  noOsSandbox: true,
  stdio: ['pipe', 'pipe', 'pipe'],
});
```

---

## Environment variable configuration

The broker reads a small set of environment variables for configuration. These are
read from the **broker's** environment (the host process), not the tool's.

| Variable | Description |
|---|---|
| `MCPARMOR_AUDIT_DIR` | Override the audit log directory. Default: `~/.mcparmor/audit/`. |
| `MCPARMOR_CONFIG` | Override the config file path. Default: `~/.mcparmor/config.json`. |
| `MCPARMOR_LOG` | Log level for broker diagnostics. Values: `error`, `warn`, `info`, `debug`. Default: `warn`. |
| `MCPARMOR_NO_OS_SANDBOX` | If set to `1`, disable Layer 2 OS sandbox globally. Equivalent to passing `--no-os-sandbox`. |

These variables are not forwarded to tool subprocesses unless explicitly declared in
the tool's `armor.json` under `env.allow`.

---

## Troubleshooting common issues

### Tool exits immediately with code 127

**Cause:** `PATH` is not available in the tool's environment. The tool's runtime
(`python3`, `node`, `npx`) cannot be found.

**Fix:** Add `"PATH"` to `env.allow` in `armor.json`:
```json
"env": {
  "allow": ["PATH", "GITHUB_TOKEN"]
}
```

The broker logs a diagnostic when it detects this exit code within the first 2
seconds of startup.

### Tool exits before first message with non-zero code

**Cause:** A required environment variable or configuration file is missing. Common
causes:
- `HOME` not in `env.allow` — tool cannot find its config directory
- Tool requires a file in a path not declared in `filesystem.read`
- Token or API key not in `env.allow`

**Fix:** Check the broker's stderr for diagnostics:
```bash
mcparmor run --armor armor.json -- python3 tool.py 2>&1 | head -20
```

Then add the missing variable or path to the manifest.

### Broker returns `-32001` (capability violation) for a path the tool should be able to read

**Cause:** The path in the JSON-RPC request parameter doesn't match the glob in
`filesystem.read`. Common causes:
- Symlinks that resolve outside the declared path
- Relative paths that Layer 1 cannot normalize
- The tool passes a path through a JSON-RPC parameter that is constructed differently
  than expected

**Fix:** Run `mcparmor validate armor.json` to check the declared patterns. Test
with `mcparmor run --armor armor.json --log-level debug -- <tool>` to see which
parameter value triggered the violation.

### OS sandbox fails to apply (`-32006`)

**Cause:** The generated OS sandbox profile is invalid or the sandbox primitive is
unavailable on this kernel version.

**Fix:**
1. Check the broker's stderr for the specific sandbox error message.
2. Run `mcparmor status` to see what Layer 2 enforcement is available on the current
   system.
3. If Layer 2 is unavailable (old kernel, restrictive container), use `--no-os-sandbox`
   to fall back to Layer 1 only. Document this as a known limitation.

### Network calls blocked that should be allowed

**Cause:** The `network.allow` entry doesn't match what the tool is requesting.

**Fix:**
1. Verify the entry format is `host:port` with no scheme prefix (`https://` is not
   valid in `network.allow`).
2. Check that `deny_local` and `deny_metadata` are not blocking a legitimate address.
3. On Linux, remember that hostname enforcement is Layer 1 only — the broker blocks
   based on URL parameters, not on raw TCP connections made by the tool.

### `mcparmor` binary not found (Python SDK `ArmorPopenError`)

**Cause:** The `mcparmor` binary is not on `PATH`, or the SDK cannot locate the
platform-specific bundled binary.

**Fix:**
1. Install the binary: `brew install mcparmor` (macOS) or download from the releases
   page.
2. Ensure `mcparmor` is on your `PATH`: `which mcparmor`.
3. If using a virtual environment, ensure the environment's `PATH` includes the
   system binary location.

### Testing your manifest with the TestKit

Use the MCP Armor TestKit to verify your `armor.json` policies in CI or during
development. The TestKit runs the real broker with a mock tool behind it — no
production tools required.

See [testkit.md](testkit.md) for setup, API reference, and examples.

---

### Secret scanning blocking legitimate responses (`-32002`)

**Cause:** The tool's response contains a pattern that matches the secret scanner
(e.g. a string that looks like an AWS access key or JWT token but is not a real
secret).

**Fix:**
1. If the tool legitimately produces values that match secret patterns (e.g. a
   tool that manages API keys), consider using `scan_secrets: false` for this tool
   and document the reason.
2. If the scan is in `"strict"` mode and this is a false positive, switch to
   `scan_secrets: true` (redact mode) instead of blocking the entire response.
3. Check the audit log to see which pattern matched: the audit entry includes the
   pattern name (e.g. `"pattern": "aws_access_key"`) but not the matched value.
