import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import { CopyButton } from "@/components/ui/CopyButton";
import { HOSTS } from "@/lib/data/hosts";

/** Host support table with one-command integration per host. */
export function HostSupport(): ReactNode {
  return (
    <section
      className="py-24 px-4"
      style={{ backgroundColor: "var(--color-bg-subtle)" }}
    >
      <div className="max-w-4xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center mb-12"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            Every MCP Host. One Command.
          </h2>
        </ScrollReveal>

        <div className="overflow-x-auto">
          <table className="w-full border-collapse min-w-[400px]" aria-label="MCP host integration commands">
            <thead>
              <tr
                className="text-left text-sm font-semibold border-b"
                style={{ borderColor: "var(--color-border-strong)" }}
              >
                <th className="py-3 pr-4">Host</th>
                <th className="py-3 pr-4">Command</th>
                <th className="py-3 w-16"><span className="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              {HOSTS.map((host) => (
                <tr
                  key={host.hostId}
                  className="border-b"
                  style={{ borderColor: "var(--color-border)" }}
                >
                  <td className="py-3 pr-4 font-medium">{host.name}</td>
                  <td className="py-3 pr-4">
                    <code
                      className="text-sm"
                      style={{
                        fontFamily: "var(--font-mono)",
                        color: "var(--color-text-secondary)",
                      }}
                    >
                      {host.command}
                    </code>
                  </td>
                  <td className="py-3">
                    <CopyButton text={host.command} />
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
