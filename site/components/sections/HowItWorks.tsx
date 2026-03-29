import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";

/**
 * How It Works section — broker architecture diagram + explanation.
 *
 * Two-column on desktop (diagram left, text right), single on mobile.
 * Diagram is a static placeholder until Phase 2 animates it.
 */
export function HowItWorks(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-6xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center mb-16"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            How It Works
          </h2>
        </ScrollReveal>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-12 items-start">
          <ScrollReveal direction="left">
            <BrokerDiagram />
          </ScrollReveal>

          <div className="flex flex-col gap-8">
            <ScrollReveal delay={100}>
              <ExplanationBlock
                title="MCP Host"
                text="Claude Desktop, Cursor, VS Code — any MCP host sends JSON-RPC over stdio. MCP Armor intercepts every message."
              />
            </ScrollReveal>
            <ScrollReveal delay={200}>
              <ExplanationBlock
                title="Layer 1 — Protocol"
                text="Validates paths and URLs before the tool sees them. Scans every response for leaked secrets. Enforces timeout and output size. Logs everything."
              />
            </ScrollReveal>
            <ScrollReveal delay={300}>
              <ExplanationBlock
                title="Layer 2 — OS Sandbox"
                text="Generated from armor.json at spawn time. Seatbelt on macOS, Landlock + Seccomp on Linux. Works for any language — Python, Node, Go, Rust, compiled binaries."
              />
            </ScrollReveal>
          </div>
        </div>
      </div>
    </section>
  );
}

function ExplanationBlock({ title, text }: { title: string; text: string }): ReactNode {
  return (
    <div>
      <h3
        className="font-semibold mb-2"
        style={{ fontSize: "var(--text-h2)", lineHeight: "var(--lh-h2)" }}
      >
        {title}
      </h3>
      <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        {text}
      </p>
    </div>
  );
}

/** Static broker architecture diagram — animated in Phase 2. */
function BrokerDiagram(): ReactNode {
  return (
    <div
      className="rounded-lg p-6 font-mono text-sm"
      style={{
        backgroundColor: "var(--color-bg-muted)",
        fontFamily: "var(--font-mono)",
        fontSize: "var(--text-mono)",
        lineHeight: "var(--lh-mono)",
      }}
    >
      <pre className="whitespace-pre overflow-x-auto text-xs sm:text-sm" role="img" aria-label="Diagram showing MCP Host connecting through MCP Armor Broker with Layer 1 Protocol and Layer 2 OS Sandbox to a Tool Subprocess">{`[MCP Host: Claude Desktop]
         │
         │ stdio JSON-RPC
         ▼
┌─────────────────────────┐
│   MCP Armor Broker      │
│                         │
│  Layer 1 — Protocol     │
│  · param validation     │
│  · secret scanning      │
│  · audit log            │
│                         │
│  Layer 2 — OS Sandbox   │
│  · Seatbelt (macOS)     │
│  · Landlock (Linux)     │
│  · Seccomp (spawn)      │
└─────────────────────────┘
         │
         │ stdio (restricted)
         ▼
[Tool Subprocess]
python tool.py / node tool.js`}</pre>
    </div>
  );
}
