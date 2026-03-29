import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import { ENFORCEMENT_TABLE } from "@/lib/data/enforcement-table";

/** Enforcement mechanism table showing what MCP Armor protects. */
export function ProtectionTable(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-5xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center mb-12"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            What It Protects
          </h2>
        </ScrollReveal>

        <div className="overflow-x-auto">
          <table className="w-full border-collapse min-w-[500px]" aria-label="Enforcement mechanisms and platform support">
            <thead>
              <tr
                className="text-left text-sm font-semibold border-b"
                style={{ borderColor: "var(--color-border-strong)" }}
              >
                <th className="py-3 pr-4">Capability</th>
                <th className="py-3 pr-4">Mechanism</th>
                <th className="py-3 pr-4">Reliability</th>
                <th className="py-3">Platform</th>
              </tr>
            </thead>
            <tbody>
              {ENFORCEMENT_TABLE.map((row) => (
                <tr
                  key={row.capability}
                  className="border-b"
                  style={{ borderColor: "var(--color-border)" }}
                >
                  <td className="py-3 pr-4 font-medium">{row.capability}</td>
                  <td
                    className="py-3 pr-4 text-sm"
                    style={{ color: "var(--color-text-secondary)" }}
                  >
                    {row.mechanism}
                  </td>
                  <td className="py-3 pr-4">
                    <ReliabilityBadge level={row.reliability} />
                  </td>
                  <td
                    className="py-3 text-sm"
                    style={{ color: "var(--color-text-secondary)" }}
                  >
                    {row.platform}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function ReliabilityBadge({ level }: { level: "hard" | "soft" }): ReactNode {
  const isHard = level === "hard";
  return (
    <span
      className="text-xs font-medium px-2 py-0.5 rounded-full"
      style={{
        backgroundColor: isHard ? "var(--color-blocked-bg)" : "var(--color-allowed-bg)",
        color: isHard ? "var(--color-blocked)" : "var(--color-allowed)",
      }}
    >
      {isHard ? "Hard" : "Soft"}
    </span>
  );
}
