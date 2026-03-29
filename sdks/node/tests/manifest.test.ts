import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { ArmorManifest, ManifestLoadError } from '../src/manifest.ts';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Write content to a temporary file and return its path. */
function writeTmp(content: string, filename = 'armor.json'): string {
  const dir = mkdtempSync(join(tmpdir(), 'mcparmor-test-'));
  const filePath = join(dir, filename);
  writeFileSync(filePath, content, 'utf8');
  return filePath;
}

/** Return a minimal valid armor.json object. */
function minimalManifest(): Record<string, unknown> {
  return { version: '1.0', profile: 'strict' };
}

/** Return a full armor.json object with network + filesystem config. */
function fullManifest(): Record<string, unknown> {
  return {
    version: '1.0',
    profile: 'sandboxed',
    locked: true,
    network: {
      allow: ['api.github.com:443', '*.example.com:443', 'api.example.com:*', '*:80'],
      deny_local: true,
      deny_metadata: true,
    },
    filesystem: {
      read: ['/tmp/mcparmor/*', '/data/**'],
      write: ['/tmp/mcparmor/*'],
    },
  };
}

// ---------------------------------------------------------------------------
// ArmorManifest.load — file reading
// ---------------------------------------------------------------------------

describe('ArmorManifest.load — valid file', () => {
  it('loads a minimal valid armor.json file without throwing', () => {
    const filePath = writeTmp(JSON.stringify(minimalManifest()));
    assert.doesNotThrow(() => ArmorManifest.load(filePath));
  });

  it('returns an ArmorManifest instance', () => {
    const filePath = writeTmp(JSON.stringify(minimalManifest()));
    const manifest = ArmorManifest.load(filePath);
    assert.ok(manifest instanceof ArmorManifest);
  });

  it('exposes the profile declared in the file', () => {
    const filePath = writeTmp(JSON.stringify({ version: '1.0', profile: 'network' }));
    const manifest = ArmorManifest.load(filePath);
    assert.equal(manifest.profile, 'network');
  });
});

describe('ArmorManifest.load — error cases', () => {
  it('throws ManifestLoadError when the file does not exist', () => {
    assert.throws(
      () => ArmorManifest.load('/absolutely/does/not/exist/armor.json'),
      ManifestLoadError,
    );
  });

  it('throws ManifestLoadError for invalid JSON content', () => {
    const filePath = writeTmp('{ not valid json }');
    assert.throws(() => ArmorManifest.load(filePath), ManifestLoadError);
  });

  it('throws ManifestLoadError when version field is missing', () => {
    const filePath = writeTmp(JSON.stringify({ profile: 'strict' }));
    assert.throws(() => ArmorManifest.load(filePath), ManifestLoadError);
  });

  it('throws ManifestLoadError when the file contains a JSON array (not an object)', () => {
    const filePath = writeTmp(JSON.stringify([1, 2, 3]));
    assert.throws(() => ArmorManifest.load(filePath), ManifestLoadError);
  });

  it('throws ManifestLoadError when the file contains a JSON string (not an object)', () => {
    const filePath = writeTmp(JSON.stringify('not an object'));
    assert.throws(() => ArmorManifest.load(filePath), ManifestLoadError);
  });

  it('ManifestLoadError has name "ManifestLoadError"', () => {
    try {
      ArmorManifest.load('/no/such/path.json');
    } catch (err) {
      assert.ok(err instanceof ManifestLoadError);
      assert.equal(err.name, 'ManifestLoadError');
    }
  });

  it('ManifestLoadError is an instance of Error', () => {
    const err = new ManifestLoadError('test');
    assert.ok(err instanceof Error);
  });
});

// ---------------------------------------------------------------------------
// ArmorManifest.fromObject
// ---------------------------------------------------------------------------

describe('ArmorManifest.fromObject', () => {
  it('creates an instance from a valid plain object', () => {
    const manifest = ArmorManifest.fromObject(minimalManifest());
    assert.ok(manifest instanceof ArmorManifest);
  });

  it('exposes the profile from the object', () => {
    const manifest = ArmorManifest.fromObject({ version: '1.0', profile: 'system' });
    assert.equal(manifest.profile, 'system');
  });

  it('throws ManifestLoadError when version is missing', () => {
    assert.throws(
      () => ArmorManifest.fromObject({ profile: 'strict' }),
      ManifestLoadError,
    );
  });

  it('throws ManifestLoadError when version is not a string', () => {
    assert.throws(
      () => ArmorManifest.fromObject({ version: 42, profile: 'strict' }),
      ManifestLoadError,
    );
  });

  it('throws ManifestLoadError when network is not an object', () => {
    assert.throws(
      () => ArmorManifest.fromObject({ version: '1.0', profile: 'strict', network: 'bad' }),
      ManifestLoadError,
    );
  });

  it('throws ManifestLoadError when filesystem is not an object', () => {
    assert.throws(
      () => ArmorManifest.fromObject({ version: '1.0', profile: 'strict', filesystem: true }),
      ManifestLoadError,
    );
  });
});

// ---------------------------------------------------------------------------
// profile property
// ---------------------------------------------------------------------------

describe('ArmorManifest.profile', () => {
  it('is undefined when profile field is absent', () => {
    const manifest = ArmorManifest.fromObject({ version: '1.0' });
    assert.equal(manifest.profile, undefined);
  });

  it('reflects the declared profile value', () => {
    const manifest = ArmorManifest.fromObject({ version: '1.0', profile: 'browser' });
    assert.equal(manifest.profile, 'browser');
  });
});

// ---------------------------------------------------------------------------
// isLocked
// ---------------------------------------------------------------------------

describe('ArmorManifest.isLocked', () => {
  it('returns false by default when locked field is absent', () => {
    const manifest = ArmorManifest.fromObject(minimalManifest());
    assert.equal(manifest.isLocked(), false);
  });

  it('returns false when locked is explicitly false', () => {
    const manifest = ArmorManifest.fromObject({ ...minimalManifest(), locked: false });
    assert.equal(manifest.isLocked(), false);
  });

  it('returns true when locked is explicitly true', () => {
    const manifest = ArmorManifest.fromObject({ ...minimalManifest(), locked: true });
    assert.equal(manifest.isLocked(), true);
  });
});

// ---------------------------------------------------------------------------
// allowsNetwork — allow list matching
// ---------------------------------------------------------------------------

describe('ArmorManifest.allowsNetwork — allow list', () => {
  it('returns true for an exact host:port match', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsNetwork('api.github.com', 443), true);
  });

  it('returns false for a host that is not in the allow list', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsNetwork('evil.com', 443), false);
  });

  it('returns false for a matching host but wrong port', () => {
    // fullManifest allows "api.github.com:443" and "*:80" but not any rule matching port 22
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsNetwork('api.github.com', 22), false);
  });

  it('matches a wildcard host pattern (*.example.com)', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsNetwork('sub.example.com', 443), true);
  });

  it('does not match a non-subdomain for *.example.com (bare domain)', () => {
    // The pattern "*.example.com:443" should NOT match "example.com" itself
    // because "example.com" has no subdomain prefix.
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: {
        allow: ['*.example.com:443'],
        deny_local: false,
        deny_metadata: false,
      },
    });
    // "example.com" matches "*.example.com" because our implementation allows
    // bare domain matching for the wildcard pattern. Verify exact behavior:
    // pattern "*.example.com" → suffix is ".example.com", host must end with suffix.
    // "example.com" does not end with ".example.com" → should be false.
    assert.equal(manifest.allowsNetwork('example.com', 443), false);
  });

  it('matches when both host and port are wildcards (* host rule)', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:80'], deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('anything.com', 80), true);
  });

  it('matches a wildcard port rule (host:*)', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    // fullManifest has "api.example.com:*"
    assert.equal(manifest.allowsNetwork('api.example.com', 9999), true);
  });

  it('returns false when network section is absent', () => {
    const manifest = ArmorManifest.fromObject(minimalManifest());
    assert.equal(manifest.allowsNetwork('api.github.com', 443), false);
  });

  it('returns false when network.allow is an empty array', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: [], deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('api.github.com', 443), false);
  });

  it('returns false when network has no allow field', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('api.github.com', 443), false);
  });
});

// ---------------------------------------------------------------------------
// allowsNetwork — deny_metadata
// ---------------------------------------------------------------------------

describe('ArmorManifest.allowsNetwork — deny_metadata', () => {
  it('blocks 169.254.x.x addresses when deny_metadata is true', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: {
        allow: ['*:80'],
        deny_local: false,
        deny_metadata: true,
      },
    });
    assert.equal(manifest.allowsNetwork('169.254.169.254', 80), false);
  });

  it('blocks 169.254.0.1 (any address in 169.254.0.0/16) when deny_metadata is true', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:80'], deny_local: false, deny_metadata: true },
    });
    assert.equal(manifest.allowsNetwork('169.254.0.1', 80), false);
  });

  it('blocks metadata by default when deny_metadata is absent', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:80'], deny_local: false },
    });
    assert.equal(manifest.allowsNetwork('169.254.169.254', 80), false);
  });

  it('allows 169.254.x.x when deny_metadata is explicitly false', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:80'], deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('169.254.169.254', 80), true);
  });
});

// ---------------------------------------------------------------------------
// allowsNetwork — deny_local
// ---------------------------------------------------------------------------

describe('ArmorManifest.allowsNetwork — deny_local', () => {
  it('blocks localhost when deny_local is true', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:3000'], deny_local: true, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('localhost', 3000), false);
  });

  it('blocks 127.0.0.1 when deny_local is true', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:3000'], deny_local: true, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('127.0.0.1', 3000), false);
  });

  it('blocks ::1 when deny_local is true', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:3000'], deny_local: true, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('::1', 3000), false);
  });

  it('blocks local hosts by default when deny_local is absent', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:3000'], deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('localhost', 3000), false);
  });

  it('allows localhost when deny_local is explicitly false', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['*:3000'], deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('localhost', 3000), true);
  });
});

// ---------------------------------------------------------------------------
// allowsPathRead
// ---------------------------------------------------------------------------

describe('ArmorManifest.allowsPathRead', () => {
  it('returns true for a path matching a /* pattern', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead('/tmp/mcparmor/file.txt'), true);
  });

  it('returns false for a path in a subdirectory of a /* pattern', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead('/tmp/mcparmor/subdir/file.txt'), false);
  });

  it('returns true for a path matching a /** pattern', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead('/data/deep/nested/file.txt'), true);
  });

  it('returns true for the base directory of a /** pattern', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead('/data'), true);
  });

  it('returns false for a path outside all declared read patterns', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead('/etc/passwd'), false);
  });

  it('returns false when no filesystem section is declared', () => {
    const manifest = ArmorManifest.fromObject(minimalManifest());
    assert.equal(manifest.allowsPathRead('/tmp/anything'), false);
  });

  it('returns false when filesystem.read is an empty array', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: [], write: [] },
    });
    assert.equal(manifest.allowsPathRead('/tmp/file.txt'), false);
  });

  it('returns false when filesystem has no read field', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { write: ['/tmp/mcparmor/*'] },
    });
    assert.equal(manifest.allowsPathRead('/tmp/mcparmor/file.txt'), false);
  });

  it('returns false for an empty path string', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathRead(''), false);
  });
});

// ---------------------------------------------------------------------------
// allowsPathWrite
// ---------------------------------------------------------------------------

describe('ArmorManifest.allowsPathWrite', () => {
  it('returns true for a path matching the write /* pattern', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathWrite('/tmp/mcparmor/out.txt'), true);
  });

  it('returns false for a path not in the write list', () => {
    const manifest = ArmorManifest.fromObject(fullManifest());
    assert.equal(manifest.allowsPathWrite('/data/something.txt'), false);
  });

  it('returns false when no filesystem section is declared', () => {
    const manifest = ArmorManifest.fromObject(minimalManifest());
    assert.equal(manifest.allowsPathWrite('/tmp/anything'), false);
  });

  it('returns false when filesystem.write is an empty array', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: ['/tmp/*'], write: [] },
    });
    assert.equal(manifest.allowsPathWrite('/tmp/file.txt'), false);
  });

  it('returns false when filesystem has no write field', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: ['/tmp/mcparmor/*'] },
    });
    assert.equal(manifest.allowsPathWrite('/tmp/mcparmor/file.txt'), false);
  });

  it('read permission does not imply write permission', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: ['/data/**'], write: [] },
    });
    assert.equal(manifest.allowsPathRead('/data/file.txt'), true);
    assert.equal(manifest.allowsPathWrite('/data/file.txt'), false);
  });
});

// ---------------------------------------------------------------------------
// Edge cases and boundary values
// ---------------------------------------------------------------------------

describe('ArmorManifest — edge cases', () => {
  it('handles a manifest with only a version field', () => {
    const manifest = ArmorManifest.fromObject({ version: '1.0' });
    assert.equal(manifest.profile, undefined);
    assert.equal(manifest.isLocked(), false);
    assert.equal(manifest.allowsNetwork('api.github.com', 443), false);
    assert.equal(manifest.allowsPathRead('/tmp/file'), false);
    assert.equal(manifest.allowsPathWrite('/tmp/file'), false);
  });

  it('handles malformed (non-parseable) rule entries in network.allow gracefully', () => {
    // A rule without a colon separator should be skipped, not throw.
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['nocolon', 'api.github.com:443'], deny_local: false, deny_metadata: false },
    });
    assert.equal(manifest.allowsNetwork('api.github.com', 443), true);
  });

  it('handles a very long path without throwing', () => {
    const longPath = '/tmp/' + 'a'.repeat(4096) + '/file.txt';
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: ['/tmp/**'] },
    });
    assert.equal(manifest.allowsPathRead(longPath), true);
  });

  it('handles unicode in path patterns without throwing', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      filesystem: { read: ['/tmp/\u4e2d\u6587/*'] },
    });
    assert.doesNotThrow(() => manifest.allowsPathRead('/tmp/\u4e2d\u6587/file.txt'));
  });

  it('handles unicode in network hostnames without throwing', () => {
    const manifest = ArmorManifest.fromObject({
      version: '1.0',
      network: { allow: ['\u00e9xample.com:443'], deny_local: false, deny_metadata: false },
    });
    assert.doesNotThrow(() => manifest.allowsNetwork('\u00e9xample.com', 443));
  });

  it('ManifestLoadError message includes path information', () => {
    const badPath = '/no/such/armor.json';
    try {
      ArmorManifest.load(badPath);
      assert.fail('should have thrown');
    } catch (err) {
      assert.ok(err instanceof ManifestLoadError);
      assert.ok(err.message.includes(badPath));
    }
  });

  it('ManifestLoadError message includes context for invalid JSON', () => {
    const filePath = writeTmp('{ bad json');
    try {
      ArmorManifest.load(filePath);
      assert.fail('should have thrown');
    } catch (err) {
      assert.ok(err instanceof ManifestLoadError);
      assert.ok(err.message.length > 0);
    }
  });
});
