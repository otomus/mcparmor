import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";
import { HOSTS } from "@/lib/data/hosts";

const PYTHON_SDK = `from mcparmor import armor_popen

proc = armor_popen(
    ["uvx", "mcp-server-fetch"],
    armor="profiles/community/fetch.armor.json",
)

# proc is Popen-compatible — use proc.stdin/proc.stdout as usual`;

const NODE_SDK = `import { armorSpawn } from 'mcparmor';

const proc = armorSpawn(
  ['npx', '-y', '@modelcontextprotocol/server-github'],
  { armor: 'profiles/community/github.armor.json' }
);

// proc.invoke() sends JSON-RPC calls through the broker`;

/** Host integrations and SDK usage guide. */
export default function IntegrationsPage(): ReactNode {
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
        Integrations
      </h1>

      <Section title="MCP Host Integration">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          One command per host. No per-tool changes. No config editing by hand.
        </p>
        <div className="flex flex-col gap-3">
          {HOSTS.map((host) => (
            <div key={host.hostId}>
              <p className="font-medium mb-1">{host.name}</p>
              <CodeBlock code={host.command} lang="bash" />
              <p className="mt-1 text-xs" style={{ color: "var(--color-text-tertiary)" }}>
                Config: <code>{host.configPath}</code>
              </p>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Python SDK">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Drop-in replacement for <code>subprocess.Popen</code> that applies armor
          enforcement before the process starts.
        </p>
        <CodeBlock code="pip install otomus-mcp-armor" lang="bash" />
        <div className="mt-4">
          <CodeBlock code={PYTHON_SDK} lang="python" filename="example.py" />
        </div>
      </Section>

      <Section title="Node.js SDK">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Drop-in replacement for <code>child_process.spawn</code> with armor enforcement.
        </p>
        <CodeBlock code="npm install mcparmor" lang="bash" />
        <div className="mt-4">
          <CodeBlock code={NODE_SDK} lang="javascript" filename="example.js" />
        </div>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }): ReactNode {
  return (
    <div className="mt-10">
      <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
        {title}
      </h2>
      {children}
    </div>
  );
}
