import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import { CodeBlock } from "@/components/ui/CodeBlock";

const ARMOR_JSON_SNIPPET = `{
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": ["api.github.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false
}`;

const PYTHON_SNIPPET = `from mcparmor import armor_popen

proc = armor_popen(
    ["uvx", "mcp-server-fetch"],
    armor="profiles/community/fetch.armor.json",
)`;

const NODE_SNIPPET = `import { armorSpawn } from 'mcparmor';

const proc = armorSpawn(
  ['npx', '-y', '@modelcontextprotocol/server-github'],
  { armor: 'profiles/community/github.armor.json' }
);`;

/**
 * Two-column section: tool authors (left) and runtime builders (right).
 *
 * Each column slides in from its respective side on scroll.
 */
export function ForAuthorsBuilders(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-6xl mx-auto">
        <h2 className="sr-only">For Tool Authors and Runtime Builders</h2>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-12">
          <ScrollReveal direction="left">
            <h3
              className="font-semibold mb-4"
              style={{ fontSize: "var(--text-h2)", lineHeight: "var(--lh-h2)" }}
            >
              For Tool Authors
            </h3>
            <p
              className="mb-6"
              style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}
            >
              Ship with declared capabilities. Add an <code>armor.json</code> to your
              tool and users get enforcement without configuration.
            </p>
            <CodeBlock code={ARMOR_JSON_SNIPPET} lang="json" filename="armor.json" />
          </ScrollReveal>

          <ScrollReveal direction="right">
            <h3
              className="font-semibold mb-4"
              style={{ fontSize: "var(--text-h2)", lineHeight: "var(--lh-h2)" }}
            >
              For Runtime Builders
            </h3>
            <p
              className="mb-6"
              style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}
            >
              One-line integration. Drop-in replacements for subprocess spawn that
              apply armor enforcement before the process starts.
            </p>
            <div className="flex flex-col gap-4">
              <CodeBlock code={PYTHON_SNIPPET} lang="python" filename="Python" />
              <CodeBlock code={NODE_SNIPPET} lang="javascript" filename="Node.js" />
            </div>
          </ScrollReveal>
        </div>
      </div>
    </section>
  );
}
