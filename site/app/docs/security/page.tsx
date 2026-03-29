import type { ReactNode } from "react";
import { AdversarialMatrix } from "@/components/sections/AdversarialMatrix";
import { ProtectionTable } from "@/components/sections/ProtectionTable";

/** Security model deep dive — enforcement layers + adversarial matrix. */
export default function SecurityPage(): ReactNode {
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
        Security Model
      </h1>

      <Section title="Two-Layer Enforcement">
        <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          MCP Armor enforces capability boundaries through two independent layers.
          Both layers read the same <code>armor.json</code> manifest. A gap in one
          layer does not defeat the other.
        </p>
        <div className="mt-4">
          <h3 className="font-semibold mb-2">Layer 1 — Protocol</h3>
          <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>
            Inspects every JSON-RPC message. Validates path and URL parameters before
            the tool sees them. Scans every response for leaked secrets. Enforces
            timeout and output size limits. Logs full audit trail.
          </p>
        </div>
        <div className="mt-4">
          <h3 className="font-semibold mb-2">Layer 2 — OS Sandbox</h3>
          <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>
            Generated from the manifest at spawn time. Applied to the tool subprocess
            before the first message arrives. Uses Seatbelt (macOS), Landlock + Seccomp
            (Linux). Works for any tool language.
          </p>
        </div>
      </Section>

      <div className="mt-12 -mx-4">
        <ProtectionTable />
      </div>

      <Section title="Adversarial Testing">
        <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          We test with compiled Go binaries that bypass Layer 1 entirely — direct
          syscalls, no JSON-RPC. This is the realistic adversary model.
        </p>
      </Section>

      <div className="mt-4 -mx-4">
        <AdversarialMatrix />
      </div>

      <Section title="Known Limitations">
        <ul className="list-disc pl-5 flex flex-col gap-2 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          <li>
            <strong>Linux hostname gap:</strong> Landlock TCP enforces by port, not hostname.
            A manifest declaring <code>api.github.com:443</code> on Linux allows all traffic
            to port 443. macOS Seatbelt enforces true hostname-level constraints.
          </li>
          <li>
            <strong>Windows v1:</strong> Protocol-layer protection only. Kernel-level
            enforcement via AppContainer ships in v3.
          </li>
          <li>
            <strong>Seccomp spawn blocking:</strong> Not applied to the initial exec on Linux.
            The broker must first exec the tool before Seccomp can prevent further spawns.
          </li>
          <li>
            <strong>Malicious armor.json:</strong> A tool author can declare less than needed.
            Community profile review is the mitigation, not a technical lock.
          </li>
        </ul>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }): ReactNode {
  return (
    <div className="mt-10">
      <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
        {title}
      </h2>
      {children}
    </div>
  );
}
