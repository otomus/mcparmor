import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";

interface TestSuite {
  name: string;
  count: number;
}

const SUITES: TestSuite[] = [
  { name: "Broker unit", count: 257 },
  { name: "Integration", count: 67 },
  { name: "Core", count: 120 },
  { name: "Python SDK", count: 94 },
  { name: "Node SDK", count: 115 },
];

const TOTAL = SUITES.reduce((sum, s) => sum + s.count, 0);

/**
 * Test count breakdown — shows real test data from the codebase.
 *
 * Displayed below the adversarial matrix to reinforce engineering credibility.
 * Small, factual, no hype.
 */
export function TestEvidence(): ReactNode {
  return (
    <ScrollReveal className="py-8 px-4">
      <div className="max-w-3xl mx-auto text-center">
        <p
          className="font-semibold"
          style={{ fontSize: "var(--text-h2)", color: "var(--color-text-primary)" }}
        >
          {TOTAL} tests passing
        </p>
        <p
          className="mt-2 text-sm"
          style={{ color: "var(--color-text-tertiary)" }}
        >
          {SUITES.map((s) => `${s.count} ${s.name}`).join(" · ")}
        </p>
      </div>
    </ScrollReveal>
  );
}
