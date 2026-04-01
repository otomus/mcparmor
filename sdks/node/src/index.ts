/**
 * MCP Armor Node.js SDK.
 *
 * Allows Node.js-based MCP tool runtimes to spawn tools under MCP Armor
 * enforcement. The SDK locates the platform-specific mcparmor binary and
 * wraps the tool command so that both Layer 1 (protocol) and Layer 2 (OS)
 * enforcement are applied transparently.
 *
 * @example
 * ```typescript
 * import { armorSpawn } from 'mcparmor';
 * const proc = armorSpawn(['node', '/path/to/tool'], { armor: './armor.json' });
 * ```
 */

export { armorSpawn, type ArmorSpawnOptions } from './spawn.ts';
export { BinaryNotFoundError, BinaryChecksumError } from './binary.ts';
export { ArmorManifest, ManifestLoadError } from './manifest.ts';
export { ArmoredProcess, ArmoredProcessError, type ArmoredProcessOptions } from './process.ts';
export { ArmoredPool, ArmoredPoolError, type ArmoredPoolOptions } from './pool.ts';
export { setLogger, getLogger, type Logger } from './logger.ts';
export {
  ArmorTestHarness,
  ArmorTestHarnessError,
  ArmorErrorCode,
  type ArmorTestHarnessOptions,
  type ToolCallResult,
} from './testkit.ts';
