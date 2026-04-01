import type { ReactNode } from "react";
import { CodeBlock } from "@/components/ui/CodeBlock";

const PYTHON_QUICK_START = `import pytest
from mcparmor import ArmorTestHarness, ArmorErrorCode

@pytest.fixture
async def harness():
    async with ArmorTestHarness(armor="./armor.json") as h:
        yield h

async def test_blocked_path(harness):
    result = await harness.call_tool("read_file", {"path": "/etc/passwd"})
    assert result.blocked
    assert result.error_code == ArmorErrorCode.PATH_VIOLATION

async def test_allowed_path(harness):
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "file contents here"}]
    })
    result = await harness.call_tool("read_file", {"path": "/tmp/mcparmor/data.txt"})
    assert result.allowed
    assert result.text == "file contents here"`;

const NODE_QUICK_START = `import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { ArmorTestHarness, ArmorErrorCode } from '@otomus/mcparmor';

describe('armor.json policy tests', () => {
  let harness;

  before(async () => {
    harness = await ArmorTestHarness.start({ armor: './armor.json' });
  });

  after(async () => {
    await harness.stop();
  });

  it('blocks reads outside declared paths', async () => {
    const result = await harness.callTool('read_file', { path: '/etc/passwd' });
    assert.ok(result.blocked);
    assert.strictEqual(result.errorCode, ArmorErrorCode.PATH_VIOLATION);
  });

  it('allows reads within declared paths', async () => {
    harness.mockToolResponse({
      content: [{ type: 'text', text: 'file contents here' }],
    });
    const result = await harness.callTool('read_file', {
      path: '/tmp/mcparmor/data.txt',
    });
    assert.ok(result.allowed);
    assert.strictEqual(result.text, 'file contents here');
  });
});`;

const MOCK_RESPONSE_PYTHON = `# Default response for all tools
harness.mock_tool_response({
    "content": [{"type": "text", "text": "hello world"}]
})

# Response for a specific tool name
harness.mock_tool_response(
    {"content": [{"type": "text", "text": "issue #42"}]},
    tool_name="get_issue",
)`;

const MOCK_RESPONSE_NODE = `// Default response for all tools
harness.mockToolResponse({
  content: [{ type: 'text', text: 'hello world' }],
});

// Response for a specific tool name
harness.mockToolResponse(
  { content: [{ type: 'text', text: 'issue #42' }] },
  'get_issue',
);`;

const SECRET_SCANNING_EXAMPLE = `async def test_leaked_token_blocked(harness):
    """With scan_secrets: "strict", any secret blocks the entire response."""
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "token=ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"}]
    })
    result = await harness.call_tool("get_config", {})
    assert result.blocked
    assert result.error_code == ArmorErrorCode.SECRET_BLOCKED`;

const CI_EXAMPLE = `name: Test armor.json policies
on: [push, pull_request]

jobs:
  test-policies:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install mcparmor
        run: curl -sSfL https://install.mcp-armor.com | sh
      - name: Install deps
        run: pip install otomus-mcp-armor pytest pytest-asyncio
      - name: Run policy tests
        run: pytest tests/test_armor_policies.py -v`;

/** TestKit documentation page. */
export default function TestKitPage(): ReactNode {
  return (
    <div>
      <h1
        className="mb-2"
        style={{
          fontFamily: "var(--font-display)",
          fontSize: "var(--text-h1)",
          lineHeight: "var(--lh-h1)",
        }}
      >
        TestKit
      </h1>
      <p className="mb-8" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
        Test your <code>armor.json</code> policies against the real mcparmor broker — not a
        mock or simulation.
      </p>

      <Section title="How it works">
        <Diagram />
        <p className="mt-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          The harness spawns the real <code>mcparmor</code> binary in broker mode with a
          built-in mock MCP tool server behind it. You control the tool side; the broker
          enforces your manifest against it.
        </p>
      </Section>

      <Section title="Install">
        <div className="flex flex-col gap-3">
          <div>
            <p className="font-medium mb-1">Python</p>
            <CodeBlock code="pip install otomus-mcp-armor" lang="bash" />
          </div>
          <div>
            <p className="font-medium mb-1">Node.js</p>
            <CodeBlock code="npm install @otomus/mcparmor" lang="bash" />
          </div>
        </div>
      </Section>

      <Section title="Quick Start — Python">
        <CodeBlock code={PYTHON_QUICK_START} lang="python" filename="test_armor_policies.py" />
      </Section>

      <Section title="Quick Start — Node.js">
        <CodeBlock code={NODE_QUICK_START} lang="javascript" filename="armor.test.ts" />
      </Section>

      <Section title="API Reference">
        <h3 className="font-semibold mt-4 mb-2">Error Codes</h3>
        <ErrorCodesTable />

        <h3 className="font-semibold mt-6 mb-2">ToolCallResult</h3>
        <ToolCallResultTable />
      </Section>

      <Section title="Configuring Mock Responses">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Responses can be changed between calls — the mock server re-reads its config on every request.
        </p>
        <div className="flex flex-col gap-4">
          <CodeBlock code={MOCK_RESPONSE_PYTHON} lang="python" />
          <CodeBlock code={MOCK_RESPONSE_NODE} lang="javascript" />
        </div>
      </Section>

      <Section title="Testing Secret Scanning">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Configure the mock to return responses containing secrets and verify the broker catches them.
        </p>
        <CodeBlock code={SECRET_SCANNING_EXAMPLE} lang="python" />
      </Section>

      <Section title="CI Integration">
        <p className="mb-4" style={{ color: "var(--color-text-secondary)", lineHeight: "var(--lh-body)" }}>
          Add policy tests to your CI pipeline to catch manifest regressions.
        </p>
        <CodeBlock code={CI_EXAMPLE} lang="yaml" filename=".github/workflows/test-policies.yml" />
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

function Diagram(): ReactNode {
  return (
    <div
      className="rounded-lg border p-6 font-mono text-sm"
      style={{
        backgroundColor: "var(--color-bg-subtle)",
        borderColor: "var(--color-border)",
        color: "var(--color-text-secondary)",
      }}
    >
      <pre>{`Your test code
  │
  ▼
ArmorTestHarness
  │ (spawns)
  ▼
mcparmor broker   ← real binary, enforces armor.json
  │ (stdio JSON-RPC)
  ▼
Mock tool server  ← returns your configured responses`}</pre>
    </div>
  );
}

function ErrorCodesTable(): ReactNode {
  const codes = [
    { name: "PATH_VIOLATION", value: "-32001", desc: "Filesystem path not in declared read/write paths" },
    { name: "NETWORK_VIOLATION", value: "-32002", desc: "Undeclared outbound host or port" },
    { name: "SPAWN_VIOLATION", value: "-32003", desc: "Child process spawn when spawn: false" },
    { name: "SECRET_BLOCKED", value: "-32004", desc: "Secret detected with scan_secrets: \"strict\"" },
    { name: "TIMEOUT", value: "-32005", desc: "Tool call exceeded timeout_ms" },
  ];

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm border-collapse">
        <thead>
          <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
            <th className="text-left py-2 pr-4 font-medium">Constant</th>
            <th className="text-left py-2 pr-4 font-medium">Value</th>
            <th className="text-left py-2 font-medium">Description</th>
          </tr>
        </thead>
        <tbody>
          {codes.map((c) => (
            <tr key={c.name} style={{ borderBottom: "1px solid var(--color-border)" }}>
              <td className="py-2 pr-4 font-mono" style={{ color: "var(--color-accent)" }}>{c.name}</td>
              <td className="py-2 pr-4 font-mono">{c.value}</td>
              <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>{c.desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ToolCallResultTable(): ReactNode {
  const fields = [
    { name: "blocked", type: "bool", desc: "True if the broker returned a policy violation error" },
    { name: "allowed", type: "bool", desc: "True if the call passed through to the mock tool" },
    { name: "errorCode", type: "int | null", desc: "JSON-RPC error code, or null if not blocked" },
    { name: "errorMessage", type: "string | null", desc: "Error message, or null if not blocked" },
    { name: "response", type: "object | null", desc: "Result payload from the mock tool, or null if blocked" },
    { name: "text", type: "string | null", desc: "First text content from the response" },
    { name: "raw", type: "object", desc: "Full JSON-RPC response envelope" },
  ];

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm border-collapse">
        <thead>
          <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
            <th className="text-left py-2 pr-4 font-medium">Field</th>
            <th className="text-left py-2 pr-4 font-medium">Type</th>
            <th className="text-left py-2 font-medium">Description</th>
          </tr>
        </thead>
        <tbody>
          {fields.map((f) => (
            <tr key={f.name} style={{ borderBottom: "1px solid var(--color-border)" }}>
              <td className="py-2 pr-4 font-mono" style={{ color: "var(--color-accent)" }}>{f.name}</td>
              <td className="py-2 pr-4 font-mono text-xs">{f.type}</td>
              <td className="py-2" style={{ color: "var(--color-text-secondary)" }}>{f.desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
