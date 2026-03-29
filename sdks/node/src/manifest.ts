/**
 * ArmorManifest — typed representation of an armor.json capability manifest.
 *
 * Provides synchronous load and query methods to inspect which capabilities
 * a manifest permits. Mirrors the JSON schema at
 * spec/v1.0/armor.schema.json without re-implementing broker enforcement.
 */

import { readFileSync } from 'node:fs';

// ---------------------------------------------------------------------------
// Internal manifest shape (mirrors armor.schema.json)
// ---------------------------------------------------------------------------

interface NetworkConfig {
  readonly allow?: readonly string[];
  readonly deny_local?: boolean;
  readonly deny_metadata?: boolean;
}

interface FilesystemConfig {
  readonly read?: readonly string[];
  readonly write?: readonly string[];
}

interface ManifestData {
  readonly version: string;
  readonly profile?: string;
  readonly locked?: boolean;
  readonly network?: NetworkConfig;
  readonly filesystem?: FilesystemConfig;
}

// ---------------------------------------------------------------------------
// Network allow-rule parsing
// ---------------------------------------------------------------------------

/** Parsed representation of a single "host:port" allow rule. */
interface NetworkRule {
  readonly hostPattern: string;
  readonly portPattern: string;
}

/** The loopback host identifiers that deny_local blocks. */
const LOCAL_HOSTS = new Set(['localhost', '127.0.0.1', '::1']);

/** CIDR prefix for cloud instance metadata endpoints (169.254.0.0/16). */
const METADATA_PREFIX = '169.254.';

/**
 * Parse a "host:port" allow-rule string into its constituent parts.
 *
 * @param rule - Rule string in the form HOST:PORT (e.g. "api.github.com:443").
 * @returns Parsed rule, or null if the string is malformed.
 */
function parseNetworkRule(rule: string): NetworkRule | null {
  const lastColon = rule.lastIndexOf(':');
  if (lastColon === -1) {
    return null;
  }
  return {
    hostPattern: rule.slice(0, lastColon),
    portPattern: rule.slice(lastColon + 1),
  };
}

/**
 * Test whether a hostname matches a pattern.
 *
 * Supported pattern forms:
 * - `*`              — matches any host
 * - `*.example.com`  — matches any subdomain of example.com
 * - `api.example.com`— exact match
 *
 * @param pattern - The host pattern from the allow rule.
 * @param host - The hostname to test.
 * @returns True if the host matches the pattern.
 */
function hostMatchesPattern(pattern: string, host: string): boolean {
  if (pattern === '*') {
    return true;
  }
  if (pattern.startsWith('*.')) {
    const suffix = pattern.slice(1); // ".example.com"
    return host.endsWith(suffix);
  }
  return pattern === host;
}

/**
 * Test whether a port matches a port pattern.
 *
 * @param pattern - The port pattern: a numeric string or `*`.
 * @param port - The port number to test.
 * @returns True if the port matches the pattern.
 */
function portMatchesPattern(pattern: string, port: number): boolean {
  if (pattern === '*') {
    return true;
  }
  return parseInt(pattern, 10) === port;
}

/**
 * Determine whether a host:port pair matches any rule in the allow list.
 *
 * @param rules - Parsed network allow rules.
 * @param host - Hostname to check.
 * @param port - Port number to check.
 * @returns True if at least one rule matches.
 */
function matchesAnyRule(rules: readonly NetworkRule[], host: string, port: number): boolean {
  return rules.some(
    (rule) => hostMatchesPattern(rule.hostPattern, host) && portMatchesPattern(rule.portPattern, port),
  );
}

// ---------------------------------------------------------------------------
// Filesystem path matching
// ---------------------------------------------------------------------------

/**
 * Test whether a filesystem path matches a glob pattern.
 *
 * Supports:
 * - Exact path: `/tmp/foo`
 * - Single-segment wildcard: `/tmp/mcparmor/*` (matches `/tmp/mcparmor/file` but not subdirs)
 * - Multi-segment wildcard: `/tmp/mcparmor/**` (matches any descendant)
 * - Prefix-only: `/tmp/` (trailing slash matches any path under that directory)
 *
 * @param pattern - The glob pattern declared in the manifest.
 * @param filePath - The absolute path being tested.
 * @returns True if filePath matches the pattern.
 */
function pathMatchesGlob(pattern: string, filePath: string): boolean {
  if (pattern === filePath) {
    return true;
  }

  if (pattern.endsWith('/**')) {
    const prefix = pattern.slice(0, -3);
    return filePath === prefix || filePath.startsWith(prefix + '/');
  }

  if (pattern.endsWith('/*')) {
    const dir = pattern.slice(0, -2);
    if (!filePath.startsWith(dir + '/')) {
      return false;
    }
    const remainder = filePath.slice(dir.length + 1);
    return !remainder.includes('/');
  }

  if (pattern.endsWith('/')) {
    return filePath.startsWith(pattern);
  }

  return false;
}

/**
 * Test whether a path is permitted by a list of glob patterns.
 *
 * @param patterns - Declared allow patterns.
 * @param filePath - The absolute path being tested.
 * @returns True if at least one pattern matches.
 */
function pathIsAllowed(patterns: readonly string[], filePath: string): boolean {
  return patterns.some((p) => pathMatchesGlob(p, filePath));
}

// ---------------------------------------------------------------------------
// Manifest validation
// ---------------------------------------------------------------------------

/** Thrown when an armor.json file cannot be loaded or parsed. */
export class ManifestLoadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ManifestLoadError';
  }
}

/**
 * Assert that a value is a plain object (not null, not an array).
 *
 * @param value - The value to test.
 * @returns True if value is a non-null, non-array object.
 */
function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Validate the top-level shape of parsed manifest data.
 *
 * @param data - Raw parsed object from JSON.
 * @returns Typed ManifestData.
 * @throws {ManifestLoadError} If required fields are missing or have wrong types.
 */
function validateManifestData(data: Record<string, unknown>): ManifestData {
  if (typeof data['version'] !== 'string') {
    throw new ManifestLoadError('armor.json is missing required field "version"');
  }

  const network = data['network'];
  if (network !== undefined && !isPlainObject(network)) {
    throw new ManifestLoadError('armor.json field "network" must be an object');
  }

  const filesystem = data['filesystem'];
  if (filesystem !== undefined && !isPlainObject(filesystem)) {
    throw new ManifestLoadError('armor.json field "filesystem" must be an object');
  }

  return data as unknown as ManifestData;
}

// ---------------------------------------------------------------------------
// ArmorManifest class
// ---------------------------------------------------------------------------

/**
 * A parsed and queryable representation of an `armor.json` capability manifest.
 *
 * Use {@link ArmorManifest.load} to read from disk or {@link ArmorManifest.fromObject}
 * to parse from an already-loaded plain object.
 *
 * @example
 * ```typescript
 * const manifest = ArmorManifest.load('./armor.json');
 * manifest.allowsNetwork('api.github.com', 443); // true
 * manifest.isLocked();                           // false
 * ```
 */
export class ArmorManifest {
  /** The declared capability profile (e.g. "strict", "network"). */
  readonly profile: string | undefined;

  readonly #data: ManifestData;
  readonly #networkRules: readonly NetworkRule[];

  private constructor(data: ManifestData) {
    this.#data = data;
    this.profile = data.profile;
    this.#networkRules = this.#parseNetworkRules();
  }

  /**
   * Parse all network allow rules at construction time.
   *
   * @returns Array of parsed NetworkRule objects.
   */
  #parseNetworkRules(): readonly NetworkRule[] {
    const allowList = this.#data.network?.allow ?? [];
    return allowList.reduce<NetworkRule[]>((acc, rule) => {
      const parsed = parseNetworkRule(rule);
      if (parsed !== null) {
        acc.push(parsed);
      }
      return acc;
    }, []);
  }

  /**
   * Load and parse an `armor.json` file from the given path.
   *
   * @param filePath - Path to the armor.json file to load.
   * @returns A parsed {@link ArmorManifest} instance.
   * @throws {ManifestLoadError} If the file does not exist, contains invalid
   *   JSON, or is missing the required `version` field.
   */
  static load(filePath: string): ArmorManifest {
    let raw: string;
    try {
      raw = readFileSync(filePath, 'utf8');
    } catch (err) {
      throw new ManifestLoadError(
        `Failed to read armor.json at "${filePath}": ${errorMessage(err)}`,
      );
    }

    return ArmorManifest.fromRawJson(raw, filePath);
  }

  /**
   * Parse an armor.json manifest from a raw JSON string.
   *
   * @param json - Raw JSON string content.
   * @param sourcePath - Source path used in error messages (optional).
   * @returns A parsed {@link ArmorManifest} instance.
   * @throws {ManifestLoadError} If the string is not valid JSON or missing
   *   the required `version` field.
   */
  private static fromRawJson(json: string, sourcePath: string): ArmorManifest {
    let parsed: unknown;
    try {
      parsed = JSON.parse(json);
    } catch (err) {
      throw new ManifestLoadError(
        `armor.json at "${sourcePath}" contains invalid JSON: ${errorMessage(err)}`,
      );
    }

    if (!isPlainObject(parsed)) {
      throw new ManifestLoadError(
        `armor.json at "${sourcePath}" must be a JSON object`,
      );
    }

    const data = validateManifestData(parsed);
    return new ArmorManifest(data);
  }

  /**
   * Parse an armor.json manifest from a plain JavaScript object.
   *
   * Useful when the JSON has already been deserialized (e.g. from a test fixture
   * or a configuration loader that pre-parses JSON).
   *
   * @param data - A plain object representing the manifest contents.
   * @returns A parsed {@link ArmorManifest} instance.
   * @throws {ManifestLoadError} If required fields are missing or malformed.
   */
  static fromObject(data: Record<string, unknown>): ArmorManifest {
    const validated = validateManifestData(data);
    return new ArmorManifest(validated);
  }

  /**
   * Whether the manifest's profile is locked against runtime overrides.
   *
   * When true the broker ignores any `--profile` flag at invocation time.
   *
   * @returns The value of the `locked` field, defaulting to `false`.
   */
  isLocked(): boolean {
    return this.#data.locked ?? false;
  }

  /**
   * Whether outbound connections to the given host and port are permitted.
   *
   * Deny rules are evaluated before the allow list:
   * 1. `deny_metadata: true` (default) blocks connections to 169.254.0.0/16.
   * 2. `deny_local: true` (default) blocks connections to localhost/127.0.0.1/::1.
   * 3. The connection must match at least one entry in `network.allow`.
   *
   * If the manifest has no `network` section, all connections are denied.
   *
   * @param host - The hostname or IP address to test.
   * @param port - The destination port number.
   * @returns True if the connection is permitted by the manifest.
   */
  allowsNetwork(host: string, port: number): boolean {
    const network = this.#data.network;
    if (network === undefined) {
      return false;
    }

    const denyMetadata = network.deny_metadata ?? true;
    if (denyMetadata && host.startsWith(METADATA_PREFIX)) {
      return false;
    }

    const denyLocal = network.deny_local ?? true;
    if (denyLocal && LOCAL_HOSTS.has(host)) {
      return false;
    }

    return matchesAnyRule(this.#networkRules, host, port);
  }

  /**
   * Whether read access to the given filesystem path is permitted.
   *
   * The path must match at least one pattern in `filesystem.read`.
   * If no `filesystem` section is declared, read access is denied.
   *
   * @param filePath - The absolute filesystem path to test.
   * @returns True if the path is covered by a declared read pattern.
   */
  allowsPathRead(filePath: string): boolean {
    const readPatterns = this.#data.filesystem?.read;
    if (readPatterns === undefined || readPatterns.length === 0) {
      return false;
    }
    return pathIsAllowed(readPatterns, filePath);
  }

  /**
   * Whether write access to the given filesystem path is permitted.
   *
   * The path must match at least one pattern in `filesystem.write`.
   * If no `filesystem` section is declared, write access is denied.
   *
   * @param filePath - The absolute filesystem path to test.
   * @returns True if the path is covered by a declared write pattern.
   */
  allowsPathWrite(filePath: string): boolean {
    const writePatterns = this.#data.filesystem?.write;
    if (writePatterns === undefined || writePatterns.length === 0) {
      return false;
    }
    return pathIsAllowed(writePatterns, filePath);
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Extract a string message from an unknown thrown value.
 *
 * @param err - The thrown value.
 * @returns A human-readable string.
 */
function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
