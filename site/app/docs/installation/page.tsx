import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";

/** Installation guide with all four install methods. */
export default function InstallationPage(): ReactNode {
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
        Installation
      </h1>

      <Section title="macOS (Homebrew)">
        <CodeBlock code="brew tap otomus/mcparmor https://github.com/otomus/mcparmor && brew install mcparmor" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Requires macOS 12+. Full enforcement including Seatbelt sandbox.
        </p>
      </Section>

      <Section title="Linux (curl)">
        <CodeBlock code="curl -sSfL https://install.mcp-armor.com | sh" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Downloads the latest release binary. Requires Linux kernel 5.13+ for full Landlock support.
        </p>
      </Section>

      <Section title="npm (cross-platform)">
        <CodeBlock code="npm install -g mcparmor" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Recommended for Node.js users. Binary verified with SHA-256 checksum at install.
        </p>
      </Section>

      <Section title="pip (cross-platform)">
        <CodeBlock code="pip install mcparmor" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Recommended for Python users. Binary verified with SHA-256 checksum at install.
        </p>
      </Section>

      <Section title="Verify Installation">
        <CodeBlock code="mcparmor status" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Shows the sandbox mechanism and capability flags for your platform.
        </p>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }): ReactNode {
  return (
    <div className="mt-8">
      <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
        {title}
      </h2>
      {children}
    </div>
  );
}
