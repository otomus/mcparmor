# MCP Armor — Getting Started

MCP Armor is a security broker that sits between your AI agent host and its MCP
tool subprocesses. This guide gets you from zero to protected in five minutes.

---

## 1. Install

**macOS (Homebrew):**
```bash
brew install mcparmor
mcparmor --version
```

**Linux / macOS (curl):**
```bash
curl -sSfL https://install.mcp-armor.com | sh
mcparmor --version
```

**From source (requires Rust):**
```bash
cargo install mcparmor
mcparmor --version
```

---

## 2. Wrap your first host

`mcparmor wrap` rewrites the host's config so every stdio tool runs through
the broker. It backs up the original file first.

```bash
# Preview what will change — no writes
mcparmor wrap --host claude-desktop --dry-run

# Apply
mcparmor wrap --host claude-desktop
```

Restart Claude Desktop after wrapping.

**Before (`claude_desktop_config.json`):**
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

Other supported hosts: `cursor`, `vscode`, `windsurf`. See
[integrations.md](integrations.md) for the full list and per-host details.

---

## 3. Check status

```bash
mcparmor status --host claude-desktop
```

Example output:

```
Sandbox mechanism: macOS Seatbelt
  Filesystem isolation : yes
  Spawn blocking       : yes
  Network port enforce : no
  Network host enforce : yes

HOST                 TOOL        WRAPPED    ARMOR PATH
------------------------------------------------------------------------
claude-desktop       github      yes        /home/user/.mcparmor/discovered/github.armor.json
claude-desktop       fetch       yes        /home/user/.mcparmor/discovered/fetch.armor.json
```

A `WRAPPED: yes` row means the broker is active for that tool. A `WRAPPED: no`
row means the tool is running without protection — run `mcparmor wrap` again or
check if the tool uses HTTP transport (which cannot be wrapped).

---

## 4. Validate a manifest

Before deploying a custom `armor.json`, validate it:

```bash
mcparmor validate /path/to/armor.json
```

To validate against a specific profile:

```bash
mcparmor validate --profile strict /path/to/armor.json
```

The command exits 0 on success and prints a structured error on failure.

---

## 5. For tool authors

If you are publishing an MCP tool, add an `armor.json` manifest to your tool's
directory so users get protection automatically.

Generate a starter manifest:

```bash
cd /path/to/your-tool
mcparmor init
```

`mcparmor init` asks a few questions (does your tool need network access?
filesystem access?) and writes a minimal `armor.json`. Commit it alongside
your tool.

---

## Next steps

- [manifest-spec.md](manifest-spec.md) — full reference for every `armor.json` field
- [security-model.md](security-model.md) — how the two enforcement layers work
- [integrations.md](integrations.md) — per-host setup, SDK usage, and advanced patterns
- [testkit.md](testkit.md) — test your armor.json policies against the real broker
