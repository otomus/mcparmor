/**
 * Minimal logger for the MCP Armor Node.js SDK.
 *
 * Emits structured messages to stderr so they don't interfere with
 * stdio-based MCP communication on stdout. Consumers can replace the
 * default logger via {@link setLogger}.
 */

/** Logging interface that consumers can implement to redirect SDK logs. */
export interface Logger {
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
}

const DEFAULT_PREFIX = '[mcparmor]';

/** Default logger that writes to stderr. */
const defaultLogger: Logger = {
  info(message: string, ...args: unknown[]) {
    console.error(`${DEFAULT_PREFIX} INFO: ${message}`, ...args);
  },
  warn(message: string, ...args: unknown[]) {
    console.error(`${DEFAULT_PREFIX} WARN: ${message}`, ...args);
  },
  error(message: string, ...args: unknown[]) {
    console.error(`${DEFAULT_PREFIX} ERROR: ${message}`, ...args);
  },
};

let activeLogger: Logger = defaultLogger;

/**
 * Replace the SDK-wide logger.
 *
 * @param logger - Custom logger implementation, or `null` to restore
 *   the default stderr logger.
 */
export function setLogger(logger: Logger | null): void {
  activeLogger = logger ?? defaultLogger;
}

/**
 * Return the current SDK logger.
 *
 * @returns The active Logger instance.
 */
export function getLogger(): Logger {
  return activeLogger;
}
