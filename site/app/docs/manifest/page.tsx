import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";
import { ArmorValidator } from "@/components/docs/ArmorValidator";

const FULL_EXAMPLE = `{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "min_spec": "1.0",
  "profile": "network",
  "locked": false,
  "filesystem": {
    "read": ["/tmp/mcparmor/**"],
    "write": ["/tmp/mcparmor/**"]
  },
  "network": {
    "allow": ["api.github.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false,
  "env": {
    "allow": ["GITHUB_TOKEN"]
  },
  "output": {
    "scan_secrets": true,
    "max_size_kb": 1024
  },
  "timeout_ms": 30000,
  "audit": {
    "enabled": true,
    "retention_days": 90,
    "max_size_mb": 50,
    "redact_params": true
  }
}`;

/** armor.json manifest reference. */
export default function ManifestPage(): ReactNode {
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
        armor.json Manifest Reference
      </h1>

      <p style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        The capability manifest declares what an MCP tool needs. MCP Armor enforces it.
      </p>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          Full Example
        </h2>
        <CodeBlock code={FULL_EXAMPLE} lang="json" filename="armor.json" />
      </div>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          Field Reference
        </h2>
        <FieldTable />
      </div>

      <div className="mt-8">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          Profiles
        </h2>
        <ProfileTable />
      </div>

      <div className="mt-12">
        <h2 className="font-semibold mb-3" style={{ fontSize: "var(--text-h2)" }}>
          Validate Your Manifest
        </h2>
        <p
          className="mb-4"
          style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}
        >
          Paste your armor.json below to validate it against the v1.0 schema in real time.
        </p>
        <ArmorValidator />
      </div>
    </div>
  );
}

function FieldTable(): ReactNode {
  const fields = [
    { field: "version", type: "string", required: "Yes", description: 'Schema version. Must be "1.0".' },
    { field: "profile", type: "string", required: "Yes", description: "Base profile: strict, sandboxed, network, system, browser." },
    { field: "min_spec", type: "string", required: "No", description: "Minimum broker spec version required (e.g. \"1.0\")." },
    { field: "locked", type: "boolean", required: "No", description: "If true, profile cannot be overridden via --profile flag." },
    { field: "filesystem.read", type: "string[]", required: "No", description: "Glob patterns for allowed read paths." },
    { field: "filesystem.write", type: "string[]", required: "No", description: "Glob patterns for allowed write paths." },
    { field: "network.allow", type: "string[]", required: "No", description: "Allowed host:port patterns." },
    { field: "network.deny_local", type: "boolean", required: "No", description: "Block connections to 127.0.0.0/8 and ::1. Default: true." },
    { field: "network.deny_metadata", type: "boolean", required: "No", description: "Block connections to 169.254.0.0/16. Default: true." },
    { field: "spawn", type: "boolean", required: "No", description: "Whether the tool may spawn child processes." },
    { field: "env.allow", type: "string[]", required: "No", description: "Environment variables the tool may read." },
    { field: "output.scan_secrets", type: "boolean | \"strict\"", required: "No", description: "Secret scanning mode: false (off), true (redact), \"strict\" (block)." },
    { field: "output.max_size_kb", type: "number", required: "No", description: "Maximum response size in KB (1–102400)." },
    { field: "timeout_ms", type: "number", required: "No", description: "Tool timeout in ms (100–300000)." },
    { field: "audit.enabled", type: "boolean", required: "No", description: "Enable audit logging." },
    { field: "audit.retention_days", type: "number", required: "No", description: "Days to retain audit logs." },
    { field: "audit.redact_params", type: "boolean", required: "No", description: "Omit parameter values from audit entries." },
  ];

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="text-left border-b" style={{ borderColor: "var(--color-border-strong)" }}>
            <th className="py-2 pr-3 font-semibold">Field</th>
            <th className="py-2 pr-3 font-semibold">Type</th>
            <th className="py-2 pr-3 font-semibold">Required</th>
            <th className="py-2 font-semibold">Description</th>
          </tr>
        </thead>
        <tbody>
          {fields.map((f) => (
            <tr key={f.field} className="border-b" style={{ borderColor: "var(--color-border)" }}>
              <td className="py-2 pr-3 font-mono" style={{ fontSize: "var(--text-sm)" }}>{f.field}</td>
              <td className="py-2 pr-3" style={{ color: "var(--color-text-secondary)" }}>{f.type}</td>
              <td className="py-2 pr-3" style={{ color: "var(--color-text-secondary)" }}>{f.required}</td>
              <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>{f.description}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ProfileTable(): ReactNode {
  const profiles = [
    { name: "strict", fs: "none", network: "none", spawn: "false", useCase: "Untrusted / AI-generated tools" },
    { name: "sandboxed", fs: "/tmp/mcparmor/*", network: "declared only", spawn: "false", useCase: "Community tools (default)" },
    { name: "network", fs: "none", network: "declared only", spawn: "false", useCase: "Pure API tools" },
    { name: "system", fs: "declared paths", network: "declared hosts", spawn: "false", useCase: "System/OS tools" },
    { name: "browser", fs: "/tmp/mcparmor/*", network: "localhost + declared", spawn: "false", useCase: "Browser automation" },
  ];

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="text-left border-b" style={{ borderColor: "var(--color-border-strong)" }}>
            <th className="py-2 pr-3 font-semibold">Profile</th>
            <th className="py-2 pr-3 font-semibold">Filesystem</th>
            <th className="py-2 pr-3 font-semibold">Network</th>
            <th className="py-2 pr-3 font-semibold">Spawn</th>
            <th className="py-2 font-semibold">Use Case</th>
          </tr>
        </thead>
        <tbody>
          {profiles.map((p) => (
            <tr key={p.name} className="border-b" style={{ borderColor: "var(--color-border)" }}>
              <td className="py-2 pr-3 font-mono font-medium">{p.name}</td>
              <td className="py-2 pr-3" style={{ color: "var(--color-text-secondary)" }}>{p.fs}</td>
              <td className="py-2 pr-3" style={{ color: "var(--color-text-secondary)" }}>{p.network}</td>
              <td className="py-2 pr-3" style={{ color: "var(--color-text-secondary)" }}>{p.spawn}</td>
              <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>{p.useCase}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
