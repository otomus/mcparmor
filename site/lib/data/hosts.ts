/** An MCP host supported by `mcparmor wrap`. */
export interface HostInfo {
  /** Display name. */
  name: string;
  /** Identifier used in `--host` flag. */
  hostId: string;
  /** One-line wrap command. */
  command: string;
  /** Typical config file path. */
  configPath: string;
}

/** MCP hosts supported in v1. */
export const HOSTS: HostInfo[] = [
  {
    name: "Claude Desktop",
    hostId: "claude-desktop",
    command: "mcparmor wrap --host claude-desktop",
    configPath: "~/Library/Application Support/Claude/claude_desktop_config.json",
  },
  {
    name: "Claude CLI",
    hostId: "claude-cli",
    command: "mcparmor wrap --host claude-cli",
    configPath: "~/.claude/settings.json",
  },
  {
    name: "Cursor",
    hostId: "cursor",
    command: "mcparmor wrap --host cursor",
    configPath: "~/.cursor/mcp.json",
  },
  {
    name: "VS Code",
    hostId: "vscode",
    command: "mcparmor wrap --host vscode",
    configPath: ".vscode/mcp.json",
  },
  {
    name: "Windsurf",
    hostId: "windsurf",
    command: "mcparmor wrap --host windsurf",
    configPath: "~/.codeium/windsurf/mcp_config.json",
  },
];
