# MCP Armor — Windsurf

Wrap all MCP tools in Windsurf with a single command.

## Setup

1. Install MCP Armor: `brew install mcparmor` (macOS) or `curl -sSfL https://install.mcp-armor.com | sh` (Linux)
2. Run: `mcparmor wrap --host windsurf`
3. Restart Windsurf.

## What changes

`mcparmor wrap` rewrites `~/.codeium/windsurf/mcp_config.json` so each stdio tool runs through the broker.
HTTP/remote tools are left unchanged.

## Verify

Run `mcparmor status --host windsurf` to confirm all tools are protected.
Run `./verify.sh` for an automated check.

## Config path

`~/.codeium/windsurf/mcp_config.json`

Config root key: `mcpServers`
