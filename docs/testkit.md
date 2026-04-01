# MCP Armor TestKit

The TestKit lets you test your `armor.json` policies against the **real mcparmor
broker** — not a mock or simulation. You define what responses a tool would
return; the TestKit sends `tools/call` messages through the broker and tells you
whether they were blocked, allowed, or had secrets redacted.

This is the fastest way to verify that your manifest does what you think it does
before shipping it to users.

---

## How it works

```
Your test code
  │
  ▼
ArmorTestHarness
  │ (spawns)
  ▼
mcparmor broker   ← real binary, reads your armor.json, enforces policy
  │ (stdio JSON-RPC)
  ▼
Mock tool server  ← lightweight server that returns your configured responses
```

The harness spawns the real `mcparmor` binary in broker mode with a built-in
mock MCP tool server behind it. The mock server returns whatever responses you
configure — you control the "tool side" while the broker enforces your manifest
against it.

Layer 2 (OS sandbox) is disabled by default in the TestKit because it would
interfere with the mock server process. Layer 1 (protocol enforcement) is fully
active and is what the TestKit is designed to test.

---

## Install

**Python:**
```bash
pip install mcparmor
```

**Node.js:**
```bash
npm install @otomus/mcparmor
```

The TestKit requires the `mcparmor` binary to be installed and available on
`PATH`, or bundled with the SDK package.

---

## Quick start

### Python

```python
import pytest
from mcparmor import ArmorTestHarness, ArmorErrorCode

@pytest.fixture
async def harness():
    async with ArmorTestHarness(armor="./armor.json") as h:
        yield h

async def test_blocked_path(harness):
    """Tool calls referencing /etc/passwd should be blocked."""
    result = await harness.call_tool("read_file", {"path": "/etc/passwd"})
    assert result.blocked
    assert result.error_code == ArmorErrorCode.PATH_VIOLATION

async def test_allowed_path(harness):
    """Tool calls within declared paths should pass through."""
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "file contents here"}]
    })
    result = await harness.call_tool("read_file", {"path": "/tmp/mcparmor/data.txt"})
    assert result.allowed
    assert result.text == "file contents here"
```

### Node.js

```typescript
import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { ArmorTestHarness, ArmorErrorCode } from '@otomus/mcparmor';

describe('armor.json policy tests', () => {
  let harness: ArmorTestHarness;

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
});
```

---

## API Reference

### ArmorTestHarness

The test harness manages the broker lifecycle and provides methods to configure
mock responses and send tool calls.

#### Python — creating a harness

`ArmorTestHarness` is an async context manager:

```python
async with ArmorTestHarness(
    armor="./armor.json",
    profile="strict",        # optional profile override
    no_os_sandbox=True,      # default: True (Layer 1 only)
    timeout=10.0,            # seconds, default: 10
) as harness:
    # harness is ready — broker is running, handshake complete
    ...
# broker is terminated, temp files cleaned up
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `armor` | `str \| Path` | required | Path to the `armor.json` manifest to test. |
| `profile` | `str \| None` | `None` | Override the profile declared in the manifest. |
| `no_os_sandbox` | `bool` | `True` | Disable Layer 2 OS sandbox. |
| `timeout` | `float` | `10.0` | Read timeout in seconds for broker responses. |

#### Node.js — creating a harness

`ArmorTestHarness` uses a static factory:

```typescript
const harness = await ArmorTestHarness.start({
  armor: './armor.json',
  profile: 'strict',       // optional
  noOsSandbox: true,        // default: true
  timeoutMs: 10_000,        // milliseconds, default: 10000
});

// ... run tests ...

await harness.stop();
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `armor` | `string` | required | Path to the `armor.json` manifest to test. |
| `profile` | `string` | `undefined` | Override the profile declared in the manifest. |
| `noOsSandbox` | `boolean` | `true` | Disable Layer 2 OS sandbox. |
| `timeoutMs` | `number` | `10000` | Read timeout in milliseconds for broker responses. |

---

### mockToolResponse / mock_tool_response

Configure what the mock tool returns when the broker forwards a `tools/call`.

```python
# Python — default response for all tools
harness.mock_tool_response({
    "content": [{"type": "text", "text": "hello world"}]
})

# Python — response for a specific tool name
harness.mock_tool_response(
    {"content": [{"type": "text", "text": "issue #42"}]},
    tool_name="get_issue",
)
```

```typescript
// Node.js — default response for all tools
harness.mockToolResponse({
  content: [{ type: 'text', text: 'hello world' }],
});

// Node.js — response for a specific tool name
harness.mockToolResponse(
  { content: [{ type: 'text', text: 'issue #42' }] },
  'get_issue',
);
```

Responses can be changed between test calls — the mock server re-reads its
configuration on every request.

---

### setTools / set_tools

Define the tool definitions returned by `tools/list`. Use this when your test
needs the broker to know about specific tool schemas.

```python
# Python
harness.set_tools([
    {
        "name": "read_file",
        "description": "Read a file from disk",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": ["path"],
        },
    }
])
```

```typescript
// Node.js
harness.setTools([
  {
    name: 'read_file',
    description: 'Read a file from disk',
    inputSchema: {
      type: 'object',
      properties: {
        path: { type: 'string' },
      },
      required: ['path'],
    },
  },
]);
```

---

### callTool / call_tool

Send a `tools/call` JSON-RPC message through the broker and get back a
classified result.

```python
# Python
result = await harness.call_tool("read_file", {"path": "/etc/passwd"})
```

```typescript
// Node.js
const result = await harness.callTool('read_file', { path: '/etc/passwd' });
```

Returns a `ToolCallResult` (see below).

---

### sendRaw / send_raw

Send an arbitrary JSON-RPC message for testing non-`tools/call` interactions.

```python
# Python — test tools/list
response = await harness.send_raw({
    "jsonrpc": "2.0",
    "id": 99,
    "method": "tools/list",
    "params": {},
})
assert "tools" in response.get("result", {})
```

```typescript
// Node.js
const response = await harness.sendRaw({
  jsonrpc: '2.0',
  id: 99,
  method: 'tools/list',
  params: {},
});
```

---

### ToolCallResult

The return type of `callTool` / `call_tool`. Classifies the broker's response
as blocked or allowed.

| Property | Type | Description |
|---|---|---|
| `raw` | `dict` / `Record` | The full JSON-RPC response envelope. |
| `blocked` | `bool` | `True` if the broker returned an error (policy violation). |
| `allowed` | `bool` | `True` if the call passed through to the mock tool. |
| `error_code` / `errorCode` | `int \| None` | The JSON-RPC error code, or `None`/`null` if not blocked. |
| `error_message` / `errorMessage` | `str \| None` | The error message, or `None`/`null` if not blocked. |
| `response` | `dict \| None` / `Record \| null` | The `result` payload from the mock tool, or `None`/`null` if blocked. |
| `text` | `str \| None` / `string \| null` | Convenience: the first text content from the response. |

---

### ArmorErrorCode

Named constants for the JSON-RPC error codes the broker returns on policy
violations.

| Constant | Value | Meaning |
|---|---|---|
| `PATH_VIOLATION` | `-32001` | Filesystem path not in declared `filesystem.read` or `filesystem.write`. |
| `NETWORK_VIOLATION` | `-32002` | Outbound connection to an undeclared host or port. |
| `SPAWN_VIOLATION` | `-32003` | Tool attempted to spawn a child process when `spawn: false`. |
| `SECRET_BLOCKED` | `-32004` | Secret detected in response with `scan_secrets: "strict"`. |
| `TIMEOUT` | `-32005` | Tool call exceeded the declared `timeout_ms`. |

---

## Testing patterns

### Filesystem policy

Test that your manifest correctly restricts filesystem access:

```python
async def test_read_inside_allowed_path(harness):
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "data"}]
    })
    result = await harness.call_tool("read_file", {"path": "/tmp/mcparmor/notes.txt"})
    assert result.allowed

async def test_read_outside_allowed_path(harness):
    result = await harness.call_tool("read_file", {"path": "/etc/shadow"})
    assert result.blocked
    assert result.error_code == ArmorErrorCode.PATH_VIOLATION

async def test_write_to_readonly_path(harness):
    result = await harness.call_tool("write_file", {
        "path": "/usr/local/bin/evil",
        "content": "#!/bin/sh\nrm -rf /",
    })
    assert result.blocked
```

### Network policy

Test that outbound connections are restricted to declared hosts:

```python
async def test_allowed_api_host(harness):
    harness.mock_tool_response({
        "content": [{"type": "text", "text": '{"status": "ok"}'}]
    })
    result = await harness.call_tool("http_request", {
        "url": "https://api.github.com/repos/owner/repo"
    })
    assert result.allowed

async def test_blocked_undeclared_host(harness):
    result = await harness.call_tool("http_request", {
        "url": "https://evil.example.com/exfiltrate"
    })
    assert result.blocked
    assert result.error_code == ArmorErrorCode.NETWORK_VIOLATION
```

### Secret scanning

Test that the broker catches secrets in tool responses:

```python
async def test_secret_redacted(harness):
    """With scan_secrets: true, secrets are redacted but the call succeeds."""
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "key=AKIAIOSFODNN7EXAMPLE"}]
    })
    result = await harness.call_tool("get_config", {})
    assert result.allowed
    assert "AKIAIOSFODNN7EXAMPLE" not in result.text

async def test_secret_blocked_strict(harness):
    """With scan_secrets: "strict", any secret blocks the entire response."""
    harness.mock_tool_response({
        "content": [{"type": "text", "text": "token=ghp_abc123secretvalue456"}]
    })
    result = await harness.call_tool("get_config", {})
    assert result.blocked
    assert result.error_code == ArmorErrorCode.SECRET_BLOCKED
```

### Reconfiguring mid-test

The mock server re-reads its config on every request, so you can change
responses between calls without restarting:

```python
async def test_different_responses(harness):
    harness.mock_tool_response(
        {"content": [{"type": "text", "text": "v1"}]},
        tool_name="get_version",
    )
    r1 = await harness.call_tool("get_version", {})
    assert r1.text == "v1"

    harness.mock_tool_response(
        {"content": [{"type": "text", "text": "v2"}]},
        tool_name="get_version",
    )
    r2 = await harness.call_tool("get_version", {})
    assert r2.text == "v2"
```

### Profile override

Test the same manifest under different profiles:

```python
@pytest.fixture(params=["strict", "sandboxed"])
async def harness(request):
    async with ArmorTestHarness(
        armor="./armor.json",
        profile=request.param,
    ) as h:
        yield h

async def test_behavior_varies_by_profile(harness):
    result = await harness.call_tool("read_file", {"path": "/tmp/mcparmor/data.txt"})
    # strict blocks everything; sandboxed allows /tmp/mcparmor/*
```

---

## CI integration

The TestKit runs anywhere the `mcparmor` binary is available. Add it to your CI
pipeline to catch manifest regressions:

### GitHub Actions

```yaml
name: Test armor.json policies
on: [push, pull_request]

jobs:
  test-policies:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install mcparmor
        run: curl -sSfL https://install.mcp-armor.com | sh

      - name: Install Python deps
        run: pip install mcparmor pytest pytest-asyncio

      - name: Run policy tests
        run: pytest tests/test_armor_policies.py -v
```

### Skip when binary is unavailable

If your tests run in environments where `mcparmor` may not be installed, use a
skip marker:

```python
import shutil
import pytest

pytestmark = pytest.mark.skipif(
    shutil.which("mcparmor") is None,
    reason="mcparmor binary not found",
)
```

```typescript
import { execSync } from 'node:child_process';

let hasBinary = false;
try {
  execSync('mcparmor --version', { stdio: 'ignore' });
  hasBinary = true;
} catch {}
```

---

## Full example: GitHub API tool

A complete test file for a GitHub API tool manifest:

**armor.json:**
```json
{
  "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
  "version": "1.0",
  "profile": "network",
  "network": {
    "allow": ["api.github.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "env": {
    "allow": ["GITHUB_TOKEN", "PATH"]
  },
  "output": {
    "scan_secrets": "strict"
  }
}
```

**test_github_tool.py:**
```python
"""Policy tests for the GitHub API tool manifest."""

import shutil
import pytest
from mcparmor import ArmorTestHarness, ArmorErrorCode

pytestmark = pytest.mark.skipif(
    shutil.which("mcparmor") is None,
    reason="mcparmor binary not found",
)


@pytest.fixture
async def harness():
    async with ArmorTestHarness(armor="./armor.json") as h:
        yield h


class TestNetworkPolicy:
    async def test_github_api_allowed(self, harness):
        harness.mock_tool_response({
            "content": [{"type": "text", "text": '{"id": 1}'}]
        })
        result = await harness.call_tool("get_issue", {
            "url": "https://api.github.com/repos/owner/repo/issues/1"
        })
        assert result.allowed

    async def test_external_host_blocked(self, harness):
        result = await harness.call_tool("http_request", {
            "url": "https://evil.example.com/steal"
        })
        assert result.blocked
        assert result.error_code == ArmorErrorCode.NETWORK_VIOLATION

    async def test_metadata_endpoint_blocked(self, harness):
        result = await harness.call_tool("http_request", {
            "url": "http://169.254.169.254/latest/meta-data/"
        })
        assert result.blocked

    async def test_localhost_blocked(self, harness):
        result = await harness.call_tool("http_request", {
            "url": "http://localhost:8080/internal"
        })
        assert result.blocked


class TestSecretScanning:
    async def test_clean_response_passes(self, harness):
        harness.mock_tool_response({
            "content": [{"type": "text", "text": "PR #42 merged"}]
        })
        result = await harness.call_tool("get_pr", {"number": 42})
        assert result.allowed
        assert result.text == "PR #42 merged"

    async def test_leaked_token_blocked(self, harness):
        harness.mock_tool_response({
            "content": [{"type": "text", "text": "token: ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"}]
        })
        result = await harness.call_tool("get_config", {})
        assert result.blocked
        assert result.error_code == ArmorErrorCode.SECRET_BLOCKED

    async def test_aws_key_blocked(self, harness):
        harness.mock_tool_response({
            "content": [{"type": "text", "text": "key=AKIAIOSFODNN7EXAMPLE"}]
        })
        result = await harness.call_tool("get_env", {})
        assert result.blocked
        assert result.error_code == ArmorErrorCode.SECRET_BLOCKED


class TestFilesystemPolicy:
    async def test_no_filesystem_access(self, harness):
        """Network profile grants no filesystem access."""
        result = await harness.call_tool("read_file", {"path": "/tmp/data.txt"})
        assert result.blocked
        assert result.error_code == ArmorErrorCode.PATH_VIOLATION
```

---

## Next steps

- [manifest-spec.md](manifest-spec.md) — full reference for every `armor.json` field
- [security-model.md](security-model.md) — how the two enforcement layers work
- [integrations.md](integrations.md) — SDK usage and integration patterns
