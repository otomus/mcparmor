# mcparmor + VS Code (GitHub Copilot)

This showcase demonstrates how to wrap MCP tools configured for GitHub Copilot
in VS Code. VS Code stores MCP server definitions in a `.vscode/mcp.json` file
at the root of each workspace. mcparmor wraps every stdio tool in that file in
a single command.

---

## Prerequisites

- **mcparmor installed** and on your PATH:
  ```
  cargo install mcparmor
  mcparmor --version
  ```
- **VS Code** with the GitHub Copilot extension (version 1.99 or later, which
  includes MCP support).
- A `.vscode/mcp.json` file in your workspace with at least one `mcpServers`
  entry using a `"command"` field (stdio transport).

---

## Config file location

```
<workspace-root>/
  .vscode/
    mcp.json      ← VS Code reads this for MCP server definitions
```

The file is project-scoped. Each workspace has its own copy. mcparmor wraps
tools in the current directory's `.vscode/mcp.json` when you run the command
from inside that workspace.

---

## Step-by-step

### 1. Fetch community armor profiles

```sh
mcparmor profiles update
```

### 2. Navigate to your workspace

```sh
cd /Users/alice/projects/myapp
```

### 3. Dry-run to preview changes

```sh
mcparmor wrap --host vscode-project --dry-run
```

Example output:

```
[dry-run] Would wrap 'filesystem': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/filesystem.armor.json -- npx -y @modelcontextprotocol/server-filesystem /Users/alice/projects/myapp
[dry-run] Would wrap 'github': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/github.armor.json -- npx -y @modelcontextprotocol/server-github
[dry-run] Would wrap 'fetch': mcparmor run --armor /Users/alice/.mcparmor/profiles/community/fetch.armor.json -- uvx mcp-server-fetch
```

### 4. Apply the wrap

```sh
mcparmor wrap --host vscode-project
```

Expected output:

```
Wrapped 3 tool(s) in /Users/alice/projects/myapp/.vscode/mcp.json
```

### 5. Reload VS Code

VS Code picks up config changes when you reload the window. Use the command
palette (`Cmd+Shift+P`) and run **Developer: Reload Window**.

### 6. Verify

```sh
mcparmor status --host vscode-project
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
vscode-project       filesystem                yes        /Users/alice/.mcparmor/profiles/community/filesystem.armor.json
vscode-project       github                    yes        /Users/alice/.mcparmor/profiles/community/github.armor.json
vscode-project       fetch                     yes        /Users/alice/.mcparmor/profiles/community/fetch.armor.json
```

---

## What changes in the config

### Before (`.vscode/mcp.json`)

```json
{
  "servers": {
    "filesystem": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "${workspaceFolder}"]
    },
    "github": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "${env:GITHUB_PERSONAL_ACCESS_TOKEN}"
      }
    },
    "fetch": {
      "type": "stdio",
      "command": "uvx",
      "args": ["mcp-server-fetch"]
    }
  }
}
```

### After

```json
{
  "servers": {
    "filesystem": {
      "type": "stdio",
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/filesystem.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-filesystem", "${workspaceFolder}"
      ]
    },
    "github": {
      "type": "stdio",
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/github.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-github"
      ],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "${env:GITHUB_PERSONAL_ACCESS_TOKEN}"
      }
    },
    "fetch": {
      "type": "stdio",
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

VS Code variable substitutions like `${workspaceFolder}` and
`${env:GITHUB_PERSONAL_ACCESS_TOKEN}` are preserved in the `args` and `env`
fields. VS Code expands them before invoking the command; mcparmor receives the
expanded values.

---

## Committing to version control

`.vscode/mcp.json` is typically committed to the repository so all team members
get the same tool configuration. A wrapped config is safe to commit — the
`"command": "mcparmor"` entries contain only the armor path and the original
command, no secrets.

Absolute armor paths (e.g. `/Users/alice/.mcparmor/...`) are machine-specific.
If you want the config to work on any machine without modification, use the
community profile auto-discovery mechanism: omit `--armor` from the args and
ensure each developer has run `mcparmor profiles update`. The broker will find
the community profile by tool name automatically.

To write a portable wrapped config with no `--armor` paths:

```sh
mcparmor wrap --host vscode-project --no-armor-path
```

The resulting args will be:

```json
["run", "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "${workspaceFolder}"]
```

The broker resolves the armor profile at startup on each developer's machine.

---

## Reverting

```sh
mcparmor unwrap --host vscode-project
```
