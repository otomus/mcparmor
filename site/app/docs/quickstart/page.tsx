import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";

/** Quick start guide — wrap + status walkthrough. */
export default function QuickStartPage(): ReactNode {
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
        Quick Start
      </h1>

      <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        Protect every MCP tool in your Claude Desktop config in 30 seconds.
      </p>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          1. Wrap your host
        </h2>
        <CodeBlock code="mcparmor wrap --host claude-desktop" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          This rewrites your Claude Desktop config to route every stdio tool through
          the MCP Armor broker. HTTP tools are skipped with a warning.
        </p>
      </div>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          2. Restart Claude Desktop
        </h2>
        <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Close and reopen Claude Desktop. Every tool now runs under armor enforcement.
        </p>
      </div>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          3. Check status
        </h2>
        <CodeBlock code="mcparmor status --host claude-desktop" lang="bash" />
        <p className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          Shows each tool, its armor source (community profile, local file, or strict fallback),
          and the enforcement level for your platform.
        </p>
      </div>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          What just happened?
        </h2>
        <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Each tool&apos;s <code>command</code> was changed from the original binary to{" "}
          <code>mcparmor run --armor &lt;profile&gt; -- &lt;original command&gt;</code>.
          The broker intercepts every JSON-RPC message and applies an OS sandbox before
          the tool process starts. Tools with a matching community profile get their
          declared capabilities. Tools without one run under the strict profile — no
          filesystem, no network, no spawn.
        </p>
      </div>
    </div>
  );
}
