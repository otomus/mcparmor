/**
 * armorSpawn — ChildProcess wrapper with MCP Armor enforcement.
 */

import { spawn, type ChildProcess, type SpawnOptions } from 'node:child_process';
import { findBinary } from './binary.ts';

/** Error message used when the command argument is invalid. */
const ERR_INVALID_COMMAND = 'command must be a non-empty array of strings';

/** Options for armorSpawn. */
export interface ArmorSpawnOptions extends SpawnOptions {
  /** Path to the armor.json capability manifest. */
  readonly armor?: string;
  /** Override the base profile declared in armor.json. */
  readonly profile?: string;
  /** Disable OS-level sandbox (skips Layer 2 enforcement). */
  readonly noOsSandbox?: boolean;
}

/**
 * Build the mcparmor CLI flags from armor-specific options.
 *
 * @param armor - Path to armor.json, or undefined.
 * @param profile - Profile override, or undefined.
 * @param noOsSandbox - Whether to disable the OS-level sandbox.
 * @returns Array of CLI flag strings to pass before the `--` separator.
 */
function buildArmorFlags(
  armor: string | undefined,
  profile: string | undefined,
  noOsSandbox: boolean,
): string[] {
  const flags: string[] = [];

  if (armor !== undefined) {
    flags.push('--armor', armor);
  }

  if (profile !== undefined) {
    flags.push('--profile', profile);
  }

  if (noOsSandbox) {
    flags.push('--no-os-sandbox');
  }

  return flags;
}

/**
 * Extract only the standard SpawnOptions, omitting armor-specific fields.
 *
 * @param options - The full ArmorSpawnOptions object.
 * @returns A plain SpawnOptions object safe to forward to node:child_process.
 */
function extractSpawnOptions(options: ArmorSpawnOptions): SpawnOptions {
  const { armor: _armor, profile: _profile, noOsSandbox: _noOsSandbox, ...spawnOptions } = options;
  return spawnOptions;
}

/**
 * Spawn a command under MCP Armor enforcement.
 *
 * Wraps the given command with the mcparmor broker, which enforces the
 * declared capability manifest at the protocol level (Layer 1) and the
 * OS level (Layer 2) where available.
 *
 * **Encoding / text mode** — to control how stdio data is decoded, pass
 * standard Node.js `SpawnOptions` fields such as `encoding` via the
 * options object. The armor-specific fields (`armor`, `profile`,
 * `noOsSandbox`) are stripped before forwarding to `child_process.spawn`.
 *
 * @param command - The tool command and arguments to run under armor.
 *   First element is the executable; remaining elements are its arguments.
 * @param options - Armor options and standard spawn options.
 * @returns A ChildProcess whose stdio is connected to the broker, which
 *   proxies to the underlying tool.
 * @throws {TypeError} If command is not a non-empty array of strings.
 * @throws {BinaryNotFoundError} If the mcparmor binary cannot be found.
 *
 * @example
 * ```typescript
 * const proc = armorSpawn(['node', 'tool/index.js'], { armor: './armor.json' });
 * ```
 */
export function armorSpawn(
  command: readonly string[],
  options: ArmorSpawnOptions = {},
): ChildProcess {
  if (!Array.isArray(command)) {
    throw new TypeError(ERR_INVALID_COMMAND);
  }
  if (command.length === 0) {
    throw new TypeError(ERR_INVALID_COMMAND);
  }

  const binaryPath = findBinary();

  const { armor, profile, noOsSandbox = false } = options;
  const armorFlags = buildArmorFlags(armor, profile, noOsSandbox);
  const brokerArgs = ['run', ...armorFlags, '--', ...command];

  const spawnOptions = extractSpawnOptions(options);

  return spawn(binaryPath, brokerArgs, spawnOptions);
}
