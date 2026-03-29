import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import {
  ADVERSARIAL_TESTS,
  REPO_BASE_URL,
  type MatrixCell,
} from "@/lib/data/adversarial-matrix";

/**
 * Adversarial test matrix — the credibility centerpiece.
 *
 * Shows BLOCKED/ALLOWED results for compiled Go binaries that bypass
 * Layer 1. BLOCKED cells link to the blocking mechanism. ALLOWED cells
 * are documented limitations, not failures.
 */
export function AdversarialMatrix(): ReactNode {
  return (
    <section
      className="py-24 px-4"
      style={{ backgroundColor: "var(--color-bg-subtle)" }}
    >
      <div className="max-w-5xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            Here is exactly where MCP Armor protects you.
            <br />
            And where it doesn&apos;t.
          </h2>
          <p
            className="text-center mt-4 max-w-2xl mx-auto"
            style={{
              fontSize: "var(--text-body)",
              color: "var(--color-text-secondary)",
              lineHeight: "var(--lh-body)",
            }}
          >
            We tested compiled Go binaries — not just Python scripts. These are the results.
            ALLOWED cells are documented limitations, not failures. We published them anyway.
          </p>
        </ScrollReveal>

        <div className="mt-12 overflow-x-auto">
          <table className="w-full border-collapse min-w-[600px]" aria-label="Adversarial test results matrix">
            <thead>
              <tr
                className="text-left text-sm font-semibold border-b"
                style={{ borderColor: "var(--color-border-strong)" }}
              >
                <th className="py-3 pr-4 sticky left-0 z-10" style={{ backgroundColor: "var(--color-bg-subtle)" }}>
                  Test
                </th>
                <th className="py-3 pr-4">macOS</th>
                <th className="py-3 pr-4">Linux</th>
                <th className="py-3 pr-4">Blocking Layer</th>
                <th className="py-3">What It Does</th>
              </tr>
            </thead>
            <tbody>
              {ADVERSARIAL_TESTS.map((test) => (
                <tr
                  key={test.id}
                  className="border-b"
                  style={{ borderColor: "var(--color-border)" }}
                >
                  <td
                    className="py-3 pr-4 font-medium sticky left-0 z-10"
                    style={{ backgroundColor: "var(--color-bg-subtle)" }}
                  >
                    <a
                      href={`${REPO_BASE_URL}/${test.sourcePath}`}
                      className="underline"
                      target="_blank"
                      rel="noopener noreferrer"
                      aria-label={`${test.name} — view source on GitHub`}
                    >
                      {test.name}
                    </a>
                  </td>
                  <td className="py-3 pr-4">
                    <StatusCell cell={test.macos} />
                  </td>
                  <td className="py-3 pr-4">
                    <StatusCell cell={test.linux} />
                  </td>
                  <td
                    className="py-3 pr-4 text-sm"
                    style={{ color: "var(--color-text-secondary)" }}
                  >
                    {test.macos.layer}
                  </td>
                  <td
                    className="py-3 text-sm"
                    style={{ color: "var(--color-text-secondary)" }}
                  >
                    {test.description}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="mt-6 text-center">
          <a
            href="/docs/security"
            className="text-sm font-medium"
            style={{ color: "var(--color-accent)" }}
          >
            Full security model →
          </a>
        </div>
      </div>
    </section>
  );
}

function StatusCell({ cell }: { cell: MatrixCell }): ReactNode {
  if (cell.status === "blocked") {
    return (
      <span
        className="text-xs font-semibold px-2 py-1 rounded"
        style={{
          backgroundColor: "var(--color-blocked-bg)",
          color: "var(--color-blocked)",
        }}
      >
        BLOCKED
      </span>
    );
  }

  const label = cell.status === "informational" ? "INFO" : "ALLOWED";
  return (
    <span
      className="text-xs font-semibold px-2 py-1 rounded"
      style={{
        backgroundColor: "var(--color-allowed-bg)",
        color: "var(--color-allowed)",
      }}
    >
      ⚠ {label}
    </span>
  );
}
