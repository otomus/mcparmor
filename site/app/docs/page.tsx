import type { ReactNode } from "react";

interface DocCard {
  title: string;
  description: string;
  href: string;
}

const CARDS: DocCard[] = [
  {
    title: "Installation",
    description: "Install via Homebrew, curl, npm, or pip.",
    href: "/docs/installation",
  },
  {
    title: "Quick Start",
    description: "Wrap your first MCP host in 30 seconds.",
    href: "/docs/quickstart",
  },
  {
    title: "armor.json Manifest",
    description: "Full schema reference for capability declarations.",
    href: "/docs/manifest",
  },
  {
    title: "CLI Commands",
    description: "Every subcommand: run, wrap, validate, audit, and more.",
    href: "/docs/cli",
  },
  {
    title: "Security Model",
    description: "Two-layer enforcement model and adversarial test matrix.",
    href: "/docs/security",
  },
  {
    title: "Host Integrations",
    description: "Claude Desktop, Cursor, VS Code, and SDK usage.",
    href: "/docs/integrations",
  },
  {
    title: "Community Profiles",
    description: "Pre-built armor profiles for popular MCP tools.",
    href: "/docs/profiles",
  },
];

/** Docs index page with card grid. */
export default function DocsIndex(): ReactNode {
  return (
    <div>
      <h1
        className="mb-8"
        style={{
          fontFamily: "var(--font-display)",
          fontSize: "var(--text-h1)",
          lineHeight: "var(--lh-h1)",
        }}
      >
        Documentation
      </h1>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {CARDS.map((card) => (
          <a
            key={card.href}
            href={card.href}
            className="block p-6 rounded-lg border hover:border-[var(--color-accent)] transition-colors"
            style={{ borderColor: "var(--color-border)" }}
          >
            <h2 className="font-semibold mb-1">{card.title}</h2>
            <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>
              {card.description}
            </p>
          </a>
        ))}
      </div>
    </div>
  );
}
