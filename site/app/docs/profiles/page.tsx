import type { ReactNode } from "react";
import { COMMUNITY_PROFILES } from "@/lib/data/profiles";

/** Community profiles page. */
export default function ProfilesPage(): ReactNode {
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
        Community Profiles
      </h1>

      <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        Ready-to-use armor manifests for popular MCP tools. Each profile is reviewed
        by named maintainers and validated against the spec schema in CI.
      </p>

      <div className="mt-8 overflow-x-auto">
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="text-left border-b" style={{ borderColor: "var(--color-border-strong)" }}>
              <th className="py-2 pr-4 font-semibold">Profile</th>
              <th className="py-2 font-semibold">What It Allows</th>
            </tr>
          </thead>
          <tbody>
            {COMMUNITY_PROFILES.map((p) => (
              <tr key={p.name} className="border-b" style={{ borderColor: "var(--color-border)" }}>
                <td className="py-2 pr-4">
                  <a
                    href={`https://github.com/otomus/mcparmor/blob/main/profiles/community/${p.filename}`}
                    className="font-mono font-medium underline"
                    style={{ color: "var(--color-accent)" }}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {p.filename}
                  </a>
                </td>
                <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>
                  {p.description}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="mt-10">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          Contributing a Profile
        </h2>
        <ol className="list-decimal pl-5 flex flex-col gap-2 text-sm" style={{ color: "var(--color-text-secondary)" }}>
          <li>Run <code>mcparmor init</code> to generate a starter manifest.</li>
          <li>Test the tool under the profile with <code>mcparmor run --strict</code>.</li>
          <li>Add <code>_source</code>, <code>_reviewed_by</code>, and <code>_reviewed_at</code> provenance fields.</li>
          <li>Open a PR to <code>profiles/community/</code> with a capability justification.</li>
          <li>CI validates the profile against the JSON schema automatically.</li>
        </ol>
      </div>
    </div>
  );
}
