import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";

interface CommandSpec {
  name: string;
  description: string;
  usage: string;
  flags?: { flag: string; description: string }[];
}

const COMMANDS: CommandSpec[] = [
  {
    name: "run",
    description: "Run an MCP tool under armor enforcement.",
    usage: "mcparmor run [--armor <path>] [--profile <name>] [--strict] [--verbose] -- <command> [args...]",
    flags: [
      { flag: "--armor <path>", description: "Path to armor.json manifest." },
      { flag: "--profile <name>", description: "Override base profile (ignored if manifest is locked)." },
      { flag: "--strict", description: "Exit with code 2 on any violation." },
      { flag: "--verbose / -v", description: "Print ALLOW/DENY decisions to stderr." },
      { flag: "--no-os-sandbox", description: "Disable Layer 2 OS sandbox." },
      { flag: "--no-log-params", description: "Omit param values from audit entries." },
      { flag: "--audit-log <file>", description: "Write audit entries to a custom file." },
      { flag: "--no-audit", description: "Disable all audit logging." },
    ],
  },
  {
    name: "wrap",
    description: "Wrap a host config to route all stdio tools through the broker.",
    usage: "mcparmor wrap --host <name> [--config <path>] [--rewrap] [--dry-run] [--backup]",
    flags: [
      { flag: "--host <name>", description: "Target host: claude-desktop, claude-cli, cursor, vscode, windsurf." },
      { flag: "--config <path>", description: "Override host config file path." },
      { flag: "--rewrap", description: "Re-wrap already-wrapped entries." },
      { flag: "--dry-run", description: "Show changes without modifying files." },
      { flag: "--backup", description: "Create .bak copy before modifying (default: true)." },
      { flag: "--no-armor-path", description: "Omit --armor path from wrapped args (portable configs)." },
    ],
  },
  {
    name: "unwrap",
    description: "Restore a host config to its pre-wrap state.",
    usage: "mcparmor unwrap --host <name> [--config <path>]",
  },
  {
    name: "status",
    description: "Show protection state for every tool in a host config.",
    usage: "mcparmor status [--host <name>] [--format table|json]",
  },
  {
    name: "validate",
    description: "Validate an armor.json against the spec schema.",
    usage: "mcparmor validate [--armor <path>]",
  },
  {
    name: "audit",
    description: "Query and manage the audit log.",
    usage: "mcparmor audit [--tool <name>] [--event <type>] [--since <duration>] [--format table|json] [--prune] [--stats]",
    flags: [
      { flag: "--tool <name>", description: "Filter by tool name." },
      { flag: "--event <type>", description: "Filter by event type." },
      { flag: "--since <duration>", description: "Filter by time (ISO8601 or relative: 1h, 7d)." },
      { flag: "--prune", description: "Remove entries older than retention period (default: 90 days)." },
      { flag: "--stats", description: "Print log statistics." },
    ],
  },
  {
    name: "init",
    description: "Generate a minimal armor.json interactively.",
    usage: "mcparmor init [--dir <path>] [--profile <name>] [--force]",
    flags: [
      { flag: "--profile <name>", description: "Skip interactive mode, use this profile directly." },
      { flag: "--force", description: "Overwrite existing armor.json." },
    ],
  },
  {
    name: "profiles",
    description: "Manage armor profiles.",
    usage: "mcparmor profiles <list|show|update|add>",
    flags: [
      { flag: "list", description: "List all available profiles (bundled + installed)." },
      { flag: "show <name>", description: "Display the full JSON for a named profile." },
      { flag: "update", description: "Fetch latest community profiles from GitHub (SHA-256 verified)." },
      { flag: "add <file>", description: "Install a local armor.json as a named user profile." },
    ],
  },
];

/** CLI command reference page. */
export default function CLIPage(): ReactNode {
  return (
    <div>
      <h1
        className="mb-6"
        style={{
          fontFamily: "var(--font-display)",
          fontSize: "var(--text-h1)",
          lineHeight: "var(--lh-h1)",
        }}
      >
        CLI Commands
      </h1>

      <div className="flex flex-col gap-10">
        {COMMANDS.map((cmd) => (
          <CommandSection key={cmd.name} command={cmd} />
        ))}
      </div>
    </div>
  );
}

function CommandSection({ command }: { command: CommandSpec }): ReactNode {
  return (
    <div>
      <h2 className="font-semibold mb-2" style={{ fontSize: "var(--text-h2)" }}>
        mcparmor {command.name}
      </h2>
      <p className="mb-3" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        {command.description}
      </p>
      <CodeBlock code={command.usage} lang="bash" />
      {command.flags && (
        <div className="mt-4 overflow-x-auto">
          <table className="w-full border-collapse text-sm">
            <tbody>
              {command.flags.map((f) => (
                <tr key={f.flag} className="border-b" style={{ borderColor: "var(--color-border)" }}>
                  <td className="py-2 pr-4 font-mono whitespace-nowrap">{f.flag}</td>
                  <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>{f.description}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
