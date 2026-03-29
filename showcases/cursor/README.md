# mcparmor + Cursor

This showcase demonstrates wrapping MCP tools in Cursor's configuration. Cursor
supports MCP tools at two scopes: a **global** config that applies to every
project, and a **project-level** config checked into your repository. mcparmor
can wrap either or both.

---

## Prerequisites

- **mcparmor installed** and on your PATH:
  ```
  cargo install mcparmor
  mcparmor --version
  ```
- **Cursor installed** (version 0.43 or later, which includes MCP support).
- At least one MCP server configured with a `"command"` field (stdio transport).

---

## Config file locations

| Scope   | Path |
|---------|------|
| Global  | `~/.cursor/mcp.json` |
| Project | `.cursor/mcp.json` (relative to project root) |

Both files use the same `mcpServers` JSON structure. mcparmor treats them
independently — wrapping one does not affect the other.

---

## Step-by-step

### 1. Fetch community armor profiles

```sh
mcparmor profiles update
```

This pulls community-reviewed profiles for tools like `filesystem`, `github`,
and `git` into `~/.mcparmor/profiles/community/`.

### 2. Wrap the global config

```sh
mcparmor wrap --host cursor
```

This wraps `~/.cursor/mcp.json`. Expected output:

```
Wrapped 3 tool(s) in /Users/alice/.cursor/mcp.json
```

### 3. Wrap a project-level config

From inside the project directory:

```sh
mcparmor wrap --host cursor-project
```

This wraps `.cursor/mcp.json` in the current working directory.

### 4. Wrap both scopes in one step

There is no single flag that wraps both in one command — run them sequentially:

```sh
mcparmor wrap --host cursor
mcparmor wrap --host cursor-project
```

### 5. Preview changes before writing

Add `--dry-run` to either command to print what would change without touching
any files:

```sh
mcparmor wrap --host cursor --dry-run
mcparmor wrap --host cursor-project --dry-run
```

### 6. Verify

```sh
mcparmor status --host cursor
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
cursor               filesystem                yes        /Users/alice/.mcparmor/profiles/community/filesystem.armor.json
cursor               github                    yes        /Users/alice/.mcparmor/profiles/community/github.armor.json
cursor               git                       yes        /Users/alice/.mcparmor/profiles/community/git.armor.json
```

To check the project-level config:

```sh
mcparmor status --host cursor-project
```

---

## What changes in the config

### Before (`~/.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/alice/projects/myapp"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxxxxxxxxxxxxxxxxxx" }
    },
    "git": {
      "command": "uvx",
      "args": ["mcp-server-git", "--repository", "/Users/alice/projects/myapp"]
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
        "npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/alice/projects/myapp"
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
    "git": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/Users/alice/.mcparmor/profiles/community/git.armor.json",
        "--",
        "uvx", "mcp-server-git", "--repository", "/Users/alice/projects/myapp"
      ]
    }
  }
}
```

---

## Reverting

```sh
# Revert global config
mcparmor unwrap --host cursor

# Revert project-level config
mcparmor unwrap --host cursor-project
```

---

## Re-wrapping after config changes

If you add a new tool to your Cursor config after wrapping, the new entry will
not be wrapped automatically. Re-run the wrap command — already-wrapped tools
are skipped by default:

```sh
mcparmor wrap --host cursor
```

To force re-wrapping of all entries (e.g. to pick up a new community profile):

```sh
mcparmor wrap --host cursor --rewrap
```
