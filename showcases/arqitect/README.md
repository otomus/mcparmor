# mcparmor + Arqitect

This showcase demonstrates how to run MCP tools used by Arqitect agents under
mcparmor enforcement. Arqitect agents invoke MCP tools as subprocesses via the
Python SDK. By replacing bare `subprocess.Popen` calls with `armor_popen`, every
tool gets Layer 1 (JSON-RPC parameter inspection) and Layer 2 (OS-level sandbox)
enforcement with no changes to the tool itself.

---

## Prerequisites

- **mcparmor installed** and on your PATH:
  ```
  cargo install mcparmor
  mcparmor --version
  ```
- **mcparmor Python SDK** installed in your Arqitect environment:
  ```
  pip install mcparmor
  ```
- An `armor.json` for each tool you want to protect (see below).

---

## Why armor_popen instead of mcparmor wrap

`mcparmor wrap` is designed for host configs — JSON files that list tools
declaratively (Claude Desktop, Cursor, VS Code). Arqitect agents start tools
programmatically at runtime, so the integration point is the Python SDK's
`armor_popen` function rather than a config file mutation.

`armor_popen` is a drop-in replacement for `subprocess.Popen`. It prepends the
mcparmor broker to the command, then forwards all Popen keyword arguments
(stdin, stdout, env, cwd, etc.) unchanged.

---

## Example armor.json for a coding agent filesystem tool

A coding agent typically needs to read the project directory and write to a
scratch area, but should not make network calls or spawn child processes:

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "sandboxed",
  "locked": true,
  "timeout_ms": 30000,
  "filesystem": {
    "read": ["/Users/alice/projects/myapp/**"],
    "write": ["/tmp/mcparmor/*"]
  },
  "network": {
    "allow": [],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false,
  "env": {
    "allow": ["PATH", "HOME"]
  },
  "output": {
    "scan_secrets": true,
    "max_size_kb": 512
  },
  "audit": {
    "enabled": true,
    "redact_params": false
  }
}
```

Save this as `armor.json` alongside your tool, or at a stable path you can
reference at runtime (e.g. `~/.mcparmor/arqitect/filesystem-agent.armor.json`).

Key decisions for a coding agent context:

- `"locked": true` — prevents the agent from overriding the profile at runtime
  via a `--profile` flag. The restrictions the tool author declared are the
  restrictions that apply.
- `"filesystem.read"` scoped to the project path — the tool can read only the
  declared directory. Reads from `~/.ssh`, `~/.aws`, or other credential stores
  are blocked because those paths are not in the allow list.
- `"deny_metadata": true` — blocks the cloud metadata IP range (`169.254.0.0/16`)
  used by AWS, GCP, and Azure to serve instance credentials.
- `"spawn": false` — prevents the tool from shelling out to curl, git, or any
  other binary. If the tool needs git operations, use a dedicated git MCP tool
  under its own armor.

---

## Python SDK integration

### Basic usage

```python
from mcparmor import armor_popen

process = armor_popen(
    ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/alice/projects/myapp"],
    armor="/Users/alice/.mcparmor/arqitect/filesystem-agent.armor.json",
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

The resulting `process` object is a standard `subprocess.Popen` instance. Pass
it to your MCP client exactly as you would an unwrapped process.

### With a profile override (when locked is false)

```python
process = armor_popen(
    ["uvx", "mcp-server-fetch"],
    armor="./armor.json",
    profile="network",  # override the base profile declared in armor.json
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

If the armor.json sets `"locked": true`, the profile override is silently
ignored by the broker. The declared profile always wins.

### Disabling the OS sandbox for debugging

```python
process = armor_popen(
    ["python", "my_tool.py"],
    armor="./armor.json",
    no_os_sandbox=True,  # Layer 1 only — useful for diagnosing sandbox failures
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
)
```

Only use `no_os_sandbox=True` during development. In production, the OS sandbox
(Layer 2) must be active to enforce filesystem and network boundaries at the
kernel level.

### Full Arqitect agent example

```python
import subprocess
from pathlib import Path
from mcparmor import armor_popen

ARMOR_DIR = Path.home() / ".mcparmor" / "arqitect"

def start_filesystem_tool(project_path: str) -> subprocess.Popen:
    """
    Start the filesystem MCP tool under armor enforcement.

    The tool can read the project directory and write to /tmp/mcparmor/*.
    All other filesystem paths, network calls, and subprocess spawns are blocked.
    """
    return armor_popen(
        ["npx", "-y", "@modelcontextprotocol/server-filesystem", project_path],
        armor=ARMOR_DIR / "filesystem-agent.armor.json",
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

def start_github_tool() -> subprocess.Popen:
    """
    Start the GitHub MCP tool under armor enforcement.

    The tool can reach api.github.com and github.com on port 443.
    All other network destinations are blocked.
    """
    return armor_popen(
        ["npx", "-y", "@modelcontextprotocol/server-github"],
        armor=ARMOR_DIR / "github-agent.armor.json",
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={"GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx", "PATH": "/usr/bin:/usr/local/bin"},
    )
```

---

## Viewing the audit log

mcparmor writes an audit log to `~/.mcparmor/audit.jsonl` by default. Each
line is a JSON object describing a broker event (tool started, call forwarded,
call blocked, secret redacted, timeout, etc.).

```sh
# Show all events from the last hour
mcparmor audit --since 1h

# Show only parameter violations (blocked calls)
mcparmor audit --event param_violation

# Show stats across all recorded events
mcparmor audit --stats
```

Example audit output for a blocked network call:

```
TIMESTAMP                      TOOL                 EVENT                DETAIL
------------------------------------------------------------------------------------------
2026-03-28T14:22:01Z           filesystem           param_violation      path /etc/passwd outside declared read paths
2026-03-28T14:22:03Z           github               invoke               tools/call list_pull_requests
2026-03-28T14:22:04Z           github               secret_detected      output matched pattern: ghp_*
```

---

## Validating your armor.json

Before deploying, validate that your armor.json conforms to the spec:

```sh
mcparmor validate ~/.mcparmor/arqitect/filesystem-agent.armor.json
```

A valid file prints `✓ armor.json is valid` and exits 0. Errors print to stderr
and exit 1 — safe to use in CI.
