# mcparmor + Claude Desktop

This showcase demonstrates how to put every MCP tool in your Claude Desktop
configuration behind the mcparmor broker in one command. After wrapping, each
tool runs inside a two-layer enforcement boundary: JSON-RPC parameter inspection
(Layer 1) and an OS-level sandbox via macOS Seatbelt (Layer 2) — without
changing anything else about how Claude Desktop starts or communicates with tools.

---

## Prerequisites

- **mcparmor installed** and on your PATH:
  ```
  cargo install mcparmor
  # or download the pre-built binary from https://github.com/otomus/mcparmor/releases
  mcparmor --version
  ```
- **Claude Desktop installed** (version 0.7 or later).
- Your `claude_desktop_config.json` has at least one `mcpServers` entry with a
  `"command"` field (stdio transport). SSE/HTTP tools are not wrapped.

---

## Config file location

| Platform | Path |
|----------|------|
| macOS    | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Linux    | `~/.config/Claude/claude_desktop_config.json` |

---

## Step-by-step

### 1. (Optional) Fetch community armor profiles

mcparmor ships community-reviewed armor profiles for popular tools. Pull the
latest set before wrapping so the wrap command can auto-discover profiles for
tools like `filesystem`, `github`, and `fetch`:

```sh
mcparmor profiles update
```

Profiles are stored at `~/.mcparmor/profiles/community/`.

### 2. Dry-run to preview changes

```sh
mcparmor wrap --host claude-desktop --dry-run
```

Example output:

```
[dry-run] Would wrap 'filesystem': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/filesystem.armor.json -- npx -y @modelcontextprotocol/server-filesystem /Users/alice/projects
[dry-run] Would wrap 'github': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/github.armor.json -- npx -y @modelcontextprotocol/server-github
[dry-run] Would wrap 'fetch': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/fetch.armor.json -- uvx mcp-server-fetch
```

No files are written in dry-run mode.

### 3. Apply the wrap

```sh
mcparmor wrap --host claude-desktop
```

Expected output:

```
Wrapped 3 tool(s) in /Users/alice/Library/Application Support/Claude/claude_desktop_config.json
```

### 4. Restart Claude Desktop

Claude Desktop reads its config at startup. Quit and relaunch the app for the
wrapped commands to take effect.

### 5. Verify

```sh
mcparmor status --host claude-desktop
```

Example output:

```
Sandbox mechanism: macOS Seatbelt
  Filesystem isolation : yes
  Spawn blocking       : yes
  Network port enforce : no
  Network host enforce : yes

HOST                 TOOL                      WRAPPED    ARMOR PATH
--------------------------------------------------------------------------------
claude-desktop       filesystem                yes        /Users/alice/.mcparmor/profiles/community/filesystem.armor.json
claude-desktop       github                    yes        /Users/alice/.mcparmor/profiles/community/github.armor.json
claude-desktop       fetch                     yes        /Users/alice/.mcparmor/profiles/community/fetch.armor.json
```

Any tool with `WRAPPED: no` was skipped — either it uses SSE/HTTP transport (no
`"command"` field) or it was already wrapped.

---

## What changes in the config

### Before

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/alice/projects"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx" }
    },
    "fetch": {
      "command": "uvx",
      "args": ["mcp-server-fetch"]
    }
  }
}
```

### After

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/filesystem.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/alice/projects"
      ]
    },
    "github": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/github.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-github"
      ],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx" }
    },
    "fetch": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/fetch.armor.json",
        "--",
        "uvx", "mcp-server-fetch"
      ]
    }
  }
}
```

The pattern is always:

```
"command": "mcparmor"
"args": ["run", "--armor", "<armor.json path>", "--", <original command>, <original args...>]
```

The `--` separator tells the broker where the broker flags end and the tool
command begins. Everything after `--` is passed verbatim to the subprocess.

---

## Reverting

To restore the original config:

```sh
mcparmor unwrap --host claude-desktop
```

The broker extracts the original command and args from each wrapped entry and
writes them back, exactly as they were before wrapping.

---

## Writing your own armor.json

If a community profile does not exist for a tool, the broker falls back to the
`strict` profile (no filesystem, no network, no spawn). To give a tool the
permissions it needs, create an `armor.json` and point the `--armor` flag at it:

```sh
mcparmor init --profile sandboxed --dir ~/my-tool-armor
# edit ~/my-tool-armor/armor.json to add the permissions you need
mcparmor validate ~/my-tool-armor/armor.json
```

Then re-wrap with an explicit armor path:

```sh
mcparmor wrap --host claude-desktop --armor ~/my-tool-armor/armor.json
```
