/**
 * Tests for the mcparmor testkit.
 *
 * Requires the real mcparmor binary (on PATH or via MCPARMOR_BIN).
 * Spins up the broker with a mock tool server and verifies Layer 1 enforcement.
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync, mkdtempSync, rmSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { ArmorTestHarness, ArmorErrorCode, type ToolCallResult } from '../src/testkit.ts';

// ---------------------------------------------------------------------------
// Skip helpers
// ---------------------------------------------------------------------------

function binaryAvailable(): boolean {
  if (process.env['MCPARMOR_BIN'] && existsSync(process.env['MCPARMOR_BIN'])) {
    return true;
  }
  try {
    execFileSync('which', ['mcparmor'], { encoding: 'utf8' });
    return true;
  } catch {
    return false;
  }
}

const SKIP = !binaryAvailable();
const skipOpts = SKIP ? { skip: 'mcparmor binary not found' } : {};

// ---------------------------------------------------------------------------
// Armor manifest helpers
// ---------------------------------------------------------------------------

interface ArmorOptions {
  profile?: string;
  filesystem?: { read?: string[]; write?: string[] };
  network?: { allow?: string[]; deny_local?: boolean; deny_metadata?: boolean };
  output?: { scan_secrets?: boolean | string; max_size_kb?: number };
}

function writeArmor(dir: string, options: ArmorOptions = {}): string {
  const manifest: Record<string, unknown> = {
    $schema: 'https://mcp-armor.com/spec/v1.0/armor.schema.json',
    version: '1.0',
    profile: options.profile ?? 'sandboxed',
  };
  if (options.filesystem !== undefined) {
    manifest['filesystem'] = options.filesystem;
  }
  if (options.network !== undefined) {
    manifest['network'] = options.network;
  }
  if (options.output !== undefined) {
    manifest['output'] = options.output;
  }
  const armorPath = join(dir, 'armor.json');
  writeFileSync(armorPath, JSON.stringify(manifest));
  return armorPath;
}

// ---------------------------------------------------------------------------
// Lifecycle tests
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — lifecycle', skipOpts, () => {
  it('starts and stops cleanly', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir);
      const harness = await ArmorTestHarness.start({ armor });
      assert.ok(harness);
      await harness.stop();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('stop is idempotent', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir);
      const harness = await ArmorTestHarness.start({ armor });
      await harness.stop();
      await harness.stop(); // second call should not throw
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Allowed calls
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — allowed calls', skipOpts, () => {
  let dir: string;
  let harness: ArmorTestHarness;

  before(async () => {
    dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    const armor = writeArmor(dir, {
      filesystem: { read: ['/tmp/**'], write: ['/tmp/**'] },
    });
    harness = await ArmorTestHarness.start({ armor });
  });

  after(async () => {
    await harness.stop();
    rmSync(dir, { recursive: true, force: true });
  });

  it('returns mock response for allowed path', async () => {
    harness.mockToolResponse({
      content: [{ type: 'text', text: 'file contents here' }],
    });
    const result = await harness.callTool('read_file', { path: '/tmp/allowed.txt' });
    assert.equal(result.allowed, true);
    assert.equal(result.blocked, false);
    assert.equal(result.errorCode, null);
    assert.equal(result.text, 'file contents here');
  });

  it('passes through non-path arguments', async () => {
    const result = await harness.callTool('compute', { count: 42, verbose: true });
    assert.equal(result.allowed, true);
  });

  it('passes through empty arguments', async () => {
    const result = await harness.callTool('ping');
    assert.equal(result.allowed, true);
  });

  it('returns default mock response', async () => {
    harness.mockToolResponse({
      content: [{ type: 'text', text: 'default text' }],
    });
    const result = await harness.callTool('anything');
    assert.equal(result.text, 'default text');
  });
});

// ---------------------------------------------------------------------------
// Blocked calls — filesystem policy
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — blocked filesystem', skipOpts, () => {
  let dir: string;
  let harness: ArmorTestHarness;

  before(async () => {
    dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    const armor = writeArmor(dir, { profile: 'strict' });
    harness = await ArmorTestHarness.start({ armor });
  });

  after(async () => {
    await harness.stop();
    rmSync(dir, { recursive: true, force: true });
  });

  it('blocks path outside allowlist', async () => {
    const result = await harness.callTool('read_file', { path: '/etc/passwd' });
    assert.equal(result.blocked, true);
    assert.equal(result.errorCode, ArmorErrorCode.PATH_VIOLATION);
  });

  it('blocks home path', async () => {
    const result = await harness.callTool('read_file', { path: '~/Documents/secret.pdf' });
    assert.equal(result.blocked, true);
  });

  it('blocks path traversal', async () => {
    const result = await harness.callTool('read_file', { path: '../../etc/passwd' });
    assert.equal(result.blocked, true);
    assert.equal(result.errorCode, ArmorErrorCode.PATH_VIOLATION);
  });

  it('blocks percent-encoded traversal', async () => {
    const result = await harness.callTool('read_file', { path: '%2e%2e/etc/passwd' });
    assert.equal(result.blocked, true);
  });
});

// ---------------------------------------------------------------------------
// Blocked calls — network policy
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — blocked network', skipOpts, () => {
  it('blocks URL to unlisted host', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, {
        profile: 'network',
        network: { allow: ['api.github.com:443'] },
      });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        const result = await harness.callTool('fetch', { url: 'https://evil.com/exfil' });
        assert.equal(result.blocked, true);
        assert.equal(result.errorCode, ArmorErrorCode.NETWORK_VIOLATION);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('allows URL to listed host', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, {
        profile: 'network',
        network: { allow: ['api.github.com:443'] },
      });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        const result = await harness.callTool('fetch', { url: 'https://api.github.com/repos' });
        assert.equal(result.allowed, true);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Secret scanning
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — secret scanning', skipOpts, () => {
  it('blocks response with AWS key in strict mode', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, { output: { scan_secrets: 'strict' } });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        harness.mockToolResponse({
          content: [{ type: 'text', text: 'aws_access_key_id = AKIAIOSFODNN7EXAMPLE' }],
        });
        const result = await harness.callTool('get_config');
        assert.equal(result.blocked, true);
        assert.equal(result.errorCode, ArmorErrorCode.SECRET_BLOCKED);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('redacts secret in non-strict mode', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, { output: { scan_secrets: true } });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        harness.mockToolResponse({
          content: [{ type: 'text', text: 'key = AKIAIOSFODNN7EXAMPLE' }],
        });
        const result = await harness.callTool('get_config');
        assert.equal(result.allowed, true);
        assert.ok(result.text !== null);
        assert.ok(!result.text!.includes('AKIAIOSFODNN7EXAMPLE'));
        assert.ok(result.text!.includes('[REDACTED'));
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Mock reconfiguration
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — mock reconfiguration', skipOpts, () => {
  let dir: string;
  let harness: ArmorTestHarness;

  before(async () => {
    dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    const armor = writeArmor(dir, {
      filesystem: { read: ['/tmp/**'], write: ['/tmp/**'] },
    });
    harness = await ArmorTestHarness.start({ armor });
  });

  after(async () => {
    await harness.stop();
    rmSync(dir, { recursive: true, force: true });
  });

  it('response can change between calls', async () => {
    harness.mockToolResponse({
      content: [{ type: 'text', text: 'first' }],
    });
    const r1 = await harness.callTool('test_tool');
    assert.equal(r1.text, 'first');

    harness.mockToolResponse({
      content: [{ type: 'text', text: 'second' }],
    });
    const r2 = await harness.callTool('test_tool');
    assert.equal(r2.text, 'second');
  });

  it('per-tool response overrides default', async () => {
    harness.mockToolResponse({
      content: [{ type: 'text', text: 'default' }],
    });
    harness.mockToolResponse(
      { content: [{ type: 'text', text: 'specific' }] },
      'special_tool',
    );

    const rDefault = await harness.callTool('other_tool');
    assert.equal(rDefault.text, 'default');

    const rSpecific = await harness.callTool('special_tool');
    assert.equal(rSpecific.text, 'specific');
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe('ArmorTestHarness — edge cases', skipOpts, () => {
  it('deeply nested path in arguments is inspected', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, { profile: 'strict' });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        const result = await harness.callTool('process', {
          outer: { inner: { deep: '/etc/shadow' } },
        });
        assert.equal(result.blocked, true);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('non-string arguments do not trigger inspection', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir, { profile: 'strict' });
      const harness = await ArmorTestHarness.start({ armor });
      try {
        const result = await harness.callTool('compute', {
          count: 42,
          enabled: false,
          ratio: 3.14,
          tags: [1, 2, 3],
        });
        assert.equal(result.allowed, true);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('sendRaw returns tools/list response', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'testkit-'));
    try {
      const armor = writeArmor(dir);
      const harness = await ArmorTestHarness.start({ armor });
      try {
        harness.setTools([{
          name: 'my_tool',
          description: 'A test tool',
          inputSchema: { type: 'object', properties: {} },
        }]);
        const response = await harness.sendRaw({
          jsonrpc: '2.0',
          id: 999,
          method: 'tools/list',
          params: {},
        });
        assert.ok('result' in response);
        const result = response['result'] as { tools: unknown[] };
        assert.ok(result.tools.length >= 1);
      } finally {
        await harness.stop();
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
