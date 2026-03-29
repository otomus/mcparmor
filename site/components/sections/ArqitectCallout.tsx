import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";

/** Arqitect reference consumer callout card. */
export function ArqitectCallout(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-3xl mx-auto">
        <ScrollReveal>
          <div
            className="rounded-xl p-8 border-2"
            style={{
              backgroundColor: "var(--color-accent-subtle)",
              borderColor: "var(--color-accent)",
            }}
          >
            <p
              className="font-semibold mb-2"
              style={{ fontSize: "var(--text-h2)", lineHeight: "var(--lh-h2)" }}
            >
              Built by the Arqitect team
            </p>
            <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
              MCP Armor was built to solve Arqitect&apos;s own security problem.
              Arqitect is the first production consumer — every MCP tool in the
              platform runs under armor enforcement.
            </p>
            <a
              href="https://github.com/otomus/mcparmor/tree/main/showcases/arqitect"
              className="inline-block mt-4 text-sm font-medium"
              style={{ color: "var(--color-accent)" }}
              target="_blank"
              rel="noopener noreferrer"
            >
              See the integration →
            </a>
          </div>
        </ScrollReveal>
      </div>
    </section>
  );
}
