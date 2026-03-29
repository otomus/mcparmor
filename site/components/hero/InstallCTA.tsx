import type { ReactNode } from "react";
import { CopyButton } from "@/components/ui/CopyButton";

/** Two-line install CTA with copy button. */
export function InstallCTA(): ReactNode {
  const installCommand = "brew install mcparmor\nmcparmor wrap --host claude-desktop";

  return (
    <div
      className="inline-block rounded-lg p-4 mt-6"
      style={{ backgroundColor: "var(--color-bg-muted)" }}
    >
      <div className="flex items-start gap-4">
        <pre
          className="text-left"
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-mono)",
            lineHeight: "var(--lh-mono)",
          }}
        >
          <code>
            <span style={{ color: "var(--color-text-tertiary)" }}>$ </span>brew install mcparmor
            {"\n"}
            <span style={{ color: "var(--color-text-tertiary)" }}>$ </span>mcparmor wrap --host claude-desktop
          </code>
        </pre>
        <CopyButton text={installCommand} />
      </div>
    </div>
  );
}
