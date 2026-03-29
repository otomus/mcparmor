import { describe, it, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { findBinary, BinaryNotFoundError } from '../src/binary.ts';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Save and restore an env var around a test. */
function withEnv(key: string, value: string | undefined, fn: () => void): void {
  const original = process.env[key];
  if (value === undefined) {
    delete process.env[key];
  } else {
    process.env[key] = value;
  }
  try {
    fn();
  } finally {
    if (original === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = original;
    }
  }
}

// ---------------------------------------------------------------------------
// MCPARMOR_BIN env var
// ---------------------------------------------------------------------------

describe('findBinary — MCPARMOR_BIN env var', () => {
  it('returns the env var path when it points to an existing file', () => {
    // Use the node binary itself as a stand-in for a real mcparmor binary
    const nodeBin = process.execPath;
    withEnv('MCPARMOR_BIN', nodeBin, () => {
      assert.equal(findBinary(), nodeBin);
    });
  });

  it('throws BinaryNotFoundError when env var points to a non-existent path', () => {
    withEnv('MCPARMOR_BIN', '/absolutely/does/not/exist/mcparmor', () => {
      assert.throws(() => findBinary(), BinaryNotFoundError);
    });
  });

  it('ignores an empty MCPARMOR_BIN and falls through to the next strategy', () => {
    withEnv('MCPARMOR_BIN', '', () => {
      // Without a real binary installed the function should either find one on
      // PATH or throw BinaryNotFoundError — it must NOT throw TypeError or
      // use the empty string as a path.
      try {
        const result = findBinary();
        assert.ok(result.length > 0, 'should return a non-empty path');
      } catch (err) {
        assert.ok(
          err instanceof BinaryNotFoundError,
          `expected BinaryNotFoundError but got ${String(err)}`,
        );
      }
    });
  });

  it('ignores a whitespace-only MCPARMOR_BIN (treated as set but non-empty)', () => {
    // A single space IS a non-empty string — env var with " " means the user
    // set it to a space, which is not a valid path. existsSync will return
    // false and BinaryNotFoundError should be thrown.
    withEnv('MCPARMOR_BIN', ' ', () => {
      assert.throws(() => findBinary(), BinaryNotFoundError);
    });
  });
});

// ---------------------------------------------------------------------------
// BinaryNotFoundError shape
// ---------------------------------------------------------------------------

describe('BinaryNotFoundError', () => {
  it('has name "BinaryNotFoundError"', () => {
    const err = new BinaryNotFoundError();
    assert.equal(err.name, 'BinaryNotFoundError');
  });

  it('message includes installation hint mentioning a platform package', () => {
    const err = new BinaryNotFoundError();
    assert.ok(
      err.message.includes('mcparmor-linux-x64') || err.message.includes('platform package'),
      `expected installation hint in message, got: ${err.message}`,
    );
  });

  it('message includes hint about MCPARMOR_BIN env var', () => {
    const err = new BinaryNotFoundError();
    assert.ok(
      err.message.includes('MCPARMOR_BIN'),
      `expected MCPARMOR_BIN mention in message, got: ${err.message}`,
    );
  });

  it('is an instance of Error', () => {
    assert.ok(new BinaryNotFoundError() instanceof Error);
  });
});

// ---------------------------------------------------------------------------
// Fallback to PATH (integration-style — binary may or may not be present)
// ---------------------------------------------------------------------------

describe('findBinary — PATH fallback', () => {
  it('throws BinaryNotFoundError when no strategy succeeds', () => {
    // Remove env var, optional dep is not installed in test env,
    // and mcparmor is very unlikely to be on PATH in CI.
    withEnv('MCPARMOR_BIN', undefined, () => {
      try {
        findBinary();
        // If we reach here the binary was found on PATH — that's also fine.
      } catch (err) {
        assert.ok(
          err instanceof BinaryNotFoundError,
          `expected BinaryNotFoundError but got ${String(err)}`,
        );
      }
    });
  });
});
