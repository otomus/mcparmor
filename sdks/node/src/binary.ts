/**
 * Locates the mcparmor binary for the current platform.
 *
 * Search order:
 * 1. MCPARMOR_BIN environment variable (no checksum check — caller's responsibility)
 * 2. Platform-specific optional dependency (SHA256 verified against checksums.ts)
 * 3. Binary on PATH (no checksum check — comes from the user's system)
 */

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { BINARY_CHECKSUMS } from './checksums.ts';

/** Maps `${platform}-${arch}` to the corresponding optional npm package name. */
const PLATFORM_PACKAGES: Readonly<Record<string, string>> = {
  'linux-x64':    'mcparmor-linux-x64',
  'linux-arm64':  'mcparmor-linux-arm64',
  'darwin-x64':   'mcparmor-darwin-x64',
  'darwin-arm64': 'mcparmor-darwin-arm64',
  'win32-x64':    'mcparmor-win32-x64',
} as const;

/** Thrown when the mcparmor binary cannot be located by any search method. */
export class BinaryNotFoundError extends Error {
  constructor() {
    super(
      'mcparmor binary not found. Install a platform package (e.g. mcparmor-linux-x64) ' +
      'or set the MCPARMOR_BIN environment variable.',
    );
    this.name = 'BinaryNotFoundError';
  }
}

/** Thrown when the bundled binary fails SHA256 verification. */
export class BinaryChecksumError extends Error {
  constructor(binaryPath: string, expected: string, actual: string) {
    super(
      `mcparmor binary at ${binaryPath} failed SHA256 verification.\n` +
      `  Expected: ${expected}\n` +
      `  Actual  : ${actual}\n` +
      'The binary may have been tampered with. Reinstall the package to fix this.',
    );
    this.name = 'BinaryChecksumError';
  }
}

/**
 * Compute the lowercase hex SHA256 digest of a file.
 *
 * @param filePath - Absolute path to the file to hash.
 * @returns Lowercase hex SHA256 digest string.
 */
function sha256File(filePath: string): string {
  const hash = createHash('sha256');
  hash.update(readFileSync(filePath));
  return hash.digest('hex');
}

/**
 * Verify a binary's SHA256 against the expected checksum for this platform.
 *
 * Skipped when {@link BINARY_CHECKSUMS} is empty (development installs).
 *
 * @param binaryPath - Absolute path to the binary to verify.
 * @throws {BinaryChecksumError} If a checksum is registered for this platform
 *   but the binary's digest does not match.
 */
function verifyChecksum(binaryPath: string): void {
  const platformKey = `${process.platform}-${process.arch}`;
  const expected = BINARY_CHECKSUMS[platformKey];

  // No entry: empty table (dev install) or unrecognised platform — skip both.
  if (expected === undefined) {
    return;
  }

  const actual = sha256File(binaryPath);
  if (actual !== expected) {
    throw new BinaryChecksumError(binaryPath, expected, actual);
  }
}

/**
 * Resolve a path to its absolute, canonical form.
 *
 * On Unix uses `realpath`; falls back to the input string on failure.
 *
 * @param filePath - Path to resolve.
 * @returns Resolved absolute path, or the original path if resolution fails.
 */
function resolvePath(filePath: string): string {
  if (process.platform === 'win32') {
    return filePath;
  }
  try {
    return execFileSync('realpath', [filePath], { encoding: 'utf8' }).trim();
  } catch {
    return filePath;
  }
}

/**
 * Read the binary path from the MCPARMOR_BIN environment variable.
 *
 * @returns The path string, or null if the variable is absent or empty.
 */
function fromEnv(): string | null {
  const value = process.env['MCPARMOR_BIN'];
  return value != null && value.length > 0 ? value : null;
}

/**
 * Resolve the binary bundled inside the platform-specific optional dependency
 * and verify its SHA256 checksum before returning.
 *
 * Uses `require.resolve` so it respects the caller's node_modules layout.
 * Checksum verification is skipped when {@link BINARY_CHECKSUMS} is empty
 * (development installs built from source).
 *
 * @returns Absolute path to the verified binary, or null if the package is not
 *   installed.
 * @throws {BinaryChecksumError} If the binary exists but fails SHA256 verification.
 */
function fromOptionalDependency(): string | null {
  const platformKey = `${process.platform}-${process.arch}`;
  const packageName = PLATFORM_PACKAGES[platformKey];

  if (packageName === undefined) {
    return null;
  }

  let resolved: string;
  try {
    resolved = require.resolve(`${packageName}/bin/mcparmor`);
  } catch {
    return null;
  }

  const absolute = resolvePath(resolved);
  verifyChecksum(absolute);
  return absolute;
}

/**
 * Find the `mcparmor` binary on the system PATH via `which`.
 *
 * @returns Absolute path reported by `which`, or null if not found.
 */
function fromPath(): string | null {
  const whichCommand = process.platform === 'win32' ? 'where' : 'which';

  try {
    const result = execFileSync(whichCommand, ['mcparmor'], { encoding: 'utf8' });
    return result.trim().split('\n')[0]?.trim() ?? null;
  } catch {
    return null;
  }
}

/**
 * Find the mcparmor binary path.
 *
 * Searches in order: MCPARMOR_BIN env var → platform optional dependency → PATH.
 *
 * @returns Absolute path to the mcparmor binary.
 * @throws {BinaryNotFoundError} If the binary cannot be found by any method.
 */
export function findBinary(): string {
  const envPath = fromEnv();
  if (envPath !== null) {
    if (!existsSync(envPath)) {
      throw new BinaryNotFoundError();
    }
    return envPath;
  }

  const depPath = fromOptionalDependency();
  if (depPath !== null) {
    return depPath;
  }

  const pathBinary = fromPath();
  if (pathBinary !== null) {
    return pathBinary;
  }

  throw new BinaryNotFoundError();
}
