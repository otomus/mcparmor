import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import { TabSwitcher } from "@/components/ui/TabSwitcher";
import { CodeBlock } from "@/components/ui/CodeBlock";

const INSTALL_TABS = [
  {
    id: "macos",
    label: "macOS",
    content: (
      <CodeBlock
        code="brew tap otomus/mcparmor https://github.com/otomus/mcparmor && brew install mcparmor"
        lang="bash"
      />
    ),
  },
  {
    id: "linux",
    label: "Linux",
    content: (
      <CodeBlock
        code="curl -sSfL https://install.mcp-armor.com | sh"
        lang="bash"
      />
    ),
  },
  {
    id: "npm",
    label: "npm",
    content: (
      <CodeBlock code="npm install -g mcparmor" lang="bash" />
    ),
  },
  {
    id: "pip",
    label: "pip",
    content: (
      <CodeBlock code="pip install mcparmor" lang="bash" />
    ),
  },
];

/** Install section with tabbed install methods. */
export function InstallSection(): ReactNode {
  return (
    <section
      className="py-24 px-4"
      style={{ backgroundColor: "var(--color-bg-subtle)" }}
    >
      <div className="max-w-2xl mx-auto text-center">
        <ScrollReveal>
          <h2
            className="mb-8"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            Install
          </h2>
        </ScrollReveal>

        <ScrollReveal delay={100}>
          <TabSwitcher tabs={INSTALL_TABS} />
        </ScrollReveal>
      </div>
    </section>
  );
}
