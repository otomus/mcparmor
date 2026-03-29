# MCP Armor

MCP tools, armored.

Every MCP tool runs with your full OS permissions. MCP Armor enforces capability boundaries at the protocol layer and the kernel level — no trust required.

---

<!-- demo.gif: mcparmor wrap --host claude-desktop → mcparmor status → blocked attempt -->
> **Demo:** `mcparmor wrap --host claude-desktop` — one command, every tool protected.

---

## The Problem

Every MCP tool runs with your permissions — your SSH keys, your AWS credentials, your filesystem.
The MCP protocol has no capability model: a tool that claims to check the weather can read your `.env` files.
MCP Armor changes that.

---

## Install

```sh
# macOS
brew tap otomus/mcparmor https://github.com/otomus/mcparmor && brew install mcparmor

# Linux
curl -sSfL https://install.mcp-armor.com | sh

# npm (cross-platform — recommended for Node users)
npm install -g mcparmor

# pip (cross-platform — recommended for Python users)
pip install mcparmor
```

---

## Quickstart

```sh
mcparmor wrap --host claude-desktop
# Restart Claude Desktop.
mcparmor status --host claude-desktop
```

---

## What It Protects

MCP Armor enforces capability isolation through two independent layers. Both layers read the same `armor.json` manifest. A failure or gap in one layer does not defeat the other.

| Capability | Mechanism | Reliability | Platform |
|---|---|---|---|
| Env var restriction | Strips env at spawn | Hard — subprocess only gets declared vars | All |
| Param path/URL validation | Inspects JSON-RPC params before forwarding | Hard — tool never receives the call | All |
| Response secret scanning | Regex on every response | Hard — redacts before host sees it | All |
| Timeout | SIGTERM/SIGKILL on deadline | Hard | All |
| Output size limit | Truncate at max_bytes | Hard | All |
| Filesystem isolation | Landlock (Linux 5.13+) / Seatbelt (macOS) | Hard — kernel-level syscall enforcement | Linux 5.13+, macOS |
| Spawn blocking | Seccomp (Linux 3.5+) / Seatbelt (macOS) | Hard — kernel-level | Linux 3.5+, macOS |
| Network by hostname | Seatbelt (macOS) | Hard — kernel-level | macOS only |
| Network by TCP port | Landlock (Linux 6.7+) | Hard — kernel-level | Linux 6.7+ |

---

## What It Doesn't Protect

Publishing the gaps is a requirement for a credible security tool.

### Adversarial test matrix

The tests below use compiled Go binaries that bypass Layer 1 entirely — direct syscalls, no JSON-RPC. This is the realistic adversary model.

| Test | macOS | Linux | Blocking layer | What the test does |
|---|---|---|---|---|
| `path_traversal` | BLOCKED | BLOCKED | Layer 1 — path traversal detection | Sends `"path": "../../etc/passwd"` in a JSON-RPC call |
| `read_passwd` | BLOCKED | BLOCKED | Layer 1 — path not in allowlist | Sends `"path": "/etc/passwd"` — absolute path, not declared |
| `call_forbidden` | BLOCKED | BLOCKED | Layer 1 — host not in network.allow | Sends a URL to an undeclared host |
| `call_metadata` | BLOCKED | BLOCKED | Layer 1 — deny_metadata: true | Sends `http://169.254.169.254/latest/meta-data/` |
| `leak_secret` | BLOCKED | BLOCKED | Layer 2 — output secret scanning | Tool response includes a fake AWS access key |
| `spawn_child` | BLOCKED | INFORMATIONAL | Layer 2 (macOS: Seatbelt process-exec deny) | Tool calls `execvp("/bin/sh")` directly |

`INFORMATIONAL` means a known platform limitation — not a CI failure. On Linux, `spawn_child` blocking depends on whether Seccomp can be installed by the broker (kernel version and container environment). See [tests/adversarial/README.md](tests/adversarial/README.md).

### Out-of-scope threats

| Threat | Why out of scope |
|---|---|
| Prompt injection | A tool returning malicious text to hijack the AI's next action — a different layer entirely |
| Malicious `armor.json` | A tool author can declare less than they actually need — mitigated by community profile review |
| HTTP/remote MCP tools | No subprocess to wrap; enforcement is the remote provider's responsibility |
| The AI model itself | Jailbreaking, hallucination, model-level attacks — not a tool execution problem |
| Kernel CVEs | If the OS sandbox primitive has a vulnerability, MCP Armor inherits it — patch your OS |
| Tool author identity | MCP Armor does not verify who wrote or signed a tool |

The Linux hostname gap deserves specific mention: Landlock TCP (Linux 6.7+) enforces by port number, not hostname. A manifest declaring `api.github.com:443` on Linux allows all outbound traffic to port 443, not just to `api.github.com`. macOS Seatbelt enforces true hostname-level constraints. See [docs/security-model.md](docs/security-model.md) for the full platform enforcement table.

---

## How It Works

```
Any MCP Host (Claude Desktop, Cursor, VS Code…)
    ↕  JSON-RPC over stdio
[mcparmor broker]               ← Layer 1: protocol enforcement
    reads armor.json
    strips env vars at spawn
    validates params (paths, URLs)
    scans responses for secrets
    enforces timeout + size
    ↕  JSON-RPC over stdio
[OS Sandbox]                    ← Layer 2: kernel enforcement
    Landlock FS (Linux 5.13+)
    Landlock TCP (Linux 6.7+)
    Seccomp spawn (Linux 3.5+)
    Seatbelt (macOS 12+)
    ↕
<tool subprocess>               ← Python, Node, Go, Rust — anything
```

---

## Host Support

| Host | Command | Config path | Status |
|---|---|---|---|
| Claude Desktop | `mcparmor wrap --host claude-desktop` | `~/Library/Application Support/Claude/claude_desktop_config.json` | ✅ v1 |
| Claude CLI | `mcparmor wrap --host claude-cli` | `~/.claude/settings.json` | ✅ v1 |
| Cursor | `mcparmor wrap --host cursor` | `~/.cursor/mcp.json` | ✅ v1 |
| VS Code | `mcparmor wrap --host vscode` | `.vscode/mcp.json` | ✅ v1 |
| Windsurf | `mcparmor wrap --host windsurf` | `~/.codeium/windsurf/mcp_config.json` | ✅ v1 |

`mcparmor wrap` rewrites the host config in place. Before:

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx" }
    }
  }
}
```

After:

```json
{
  "mcpServers": {
    "github": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor",
        "/Users/alice/.mcparmor/profiles/community/github.armor.json",
        "--",
        "npx",
        "-y",
        "@modelcontextprotocol/server-github"
      ],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx" }
    }
  }
}
```

**Windows users:** Protocol-level protection (env stripping, param validation, secret scanning) is fully active. Kernel-level filesystem and network enforcement (Layer 2) ships in v3. Run `mcparmor status` to see exactly what's enforced on your system.

---

## For Tool Authors

Ship an `armor.json` alongside your tool and your users get enforcement without configuration. The manifest travels with the tool — reviewed by the community, enforced at the kernel level regardless of which runtime runs it.

```sh
mcparmor init
```

Generates a starter manifest. Example — the community profile for the GitHub MCP server:

```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "min_spec": "1.0",
  "profile": "network",
  "locked": false,
  "network": {
    "allow": [
      "api.github.com:443",
      "github.com:443"
    ],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false,
  "env": {
    "allow": ["GITHUB_TOKEN"]
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

## For Runtime Builders

If you are building an MCP runtime or orchestration layer, MCP Armor exposes `armor_popen` (Python) and `armorSpawn` (Node) — drop-in replacements for subprocess spawn that apply armor enforcement before the process starts. No broker process required; the enforcement runs in-process.

```python
from mcparmor import armor_popen

proc = armor_popen(
    ["uvx", "mcp-server-fetch"],
    armor="profiles/community/fetch.armor.json",
)
```

```js
import { armorSpawn } from 'mcparmor';

const proc = armorSpawn(
  ['npx', '-y', '@modelcontextprotocol/server-github'],
  { armor: 'profiles/community/github.armor.json' }
);
```

See [showcases/arqitect/](showcases/arqitect/) and [showcases/openclaw/](showcases/openclaw/) for integration examples.

---

## vs. Bulwark

> *Other tools protect the runtime. MCP Armor protects the tool. The capability declaration travels with the tool — reviewed by the community, enforced at the kernel level regardless of which runtime runs it.*

Bulwark and MCP Armor are complementary, not competing. Bulwark is a runtime operator tool: the person running the server declares what policy applies. MCP Armor is a tool-author tool: the person writing the MCP server declares what their tool actually needs. Both can be active at the same time. When they are, you get defense in depth — the tool author's declared minimums enforced by the kernel, and the operator's policy enforced at the runtime layer.

---

## Community Profiles

Ready-to-use armor manifests in [profiles/community/](profiles/community/):

| Profile | What it allows |
|---|---|
| `github.armor.json` | Outbound HTTPS to `api.github.com` and `github.com` only; env: `GITHUB_TOKEN` |
| `filesystem.armor.json` | Read/write to `/tmp/mcparmor/*` only; no network; no spawn |
| `fetch.armor.json` | Outbound HTTP/HTTPS to any host (`*:443`, `*:80`); no filesystem; no spawn |
| `git.armor.json` | Read/write to entire filesystem; spawn allowed (git forks subprocesses); no network |
| `sqlite.armor.json` | Read/write to `.db` and `.sqlite` files under `/tmp/mcparmor/` only; no network; no spawn |
| `brave-search.armor.json` | Outbound HTTPS to `api.search.brave.com` only; env: `BRAVE_API_KEY` |
| `slack.armor.json` | Outbound HTTPS to `slack.com`, `api.slack.com`, `files.slack.com`; env: `SLACK_BOT_TOKEN` |
| `notion.armor.json` | Outbound HTTPS to `api.notion.com` only; env: `NOTION_TOKEN` |
| `playwright.armor.json` | Outbound HTTP/HTTPS to any host; read access to Playwright config; spawn allowed |
| `gmail.armor.json` | Outbound HTTPS to Gmail and Google OAuth endpoints; env: OAuth credentials |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

MIT. See [LICENSE](LICENSE).
