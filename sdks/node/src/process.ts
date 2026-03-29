/**
 * ArmoredProcess — high-level wrapper for invoking MCP tools under armor enforcement.
 *
 * Manages the lifecycle of an mcparmor-brokered subprocess and provides a
 * simple JSON-RPC framing layer (line-delimited newline-terminated JSON) on
 * top of the process's stdin/stdout streams.
 */

import { type ChildProcess } from 'node:child_process';
import { armorSpawn, type ArmorSpawnOptions } from './spawn.ts';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** Options accepted by {@link ArmoredProcess}. */
export interface ArmoredProcessOptions {
  /** The tool command and arguments (first element is the executable). */
  readonly command: readonly string[];
  /** Path to the armor.json capability manifest. */
  readonly armor?: string;
  /** Override the base profile declared in armor.json. */
  readonly profile?: string;
  /** Disable OS-level sandbox (skips Layer 2 enforcement). */
  readonly noOsSandbox?: boolean;
  /** Working directory for the spawned process. */
  readonly cwd?: string;
}

/** Options accepted by {@link ArmoredProcess.invoke}. */
export interface InvokeOptions {
  /** Maximum time in milliseconds to wait for a response. Default: 30 000. */
  readonly timeoutMs?: number;
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/** Thrown on process-level errors during spawn, I/O, or JSON parsing. */
export class ArmoredProcessError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'ArmoredProcessError';
  }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_TIMEOUT_MS = 30_000;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Build the ArmorSpawnOptions from ArmoredProcessOptions.
 *
 * Omits optional fields that are `undefined` to satisfy
 * `exactOptionalPropertyTypes`.
 *
 * @param options - The ArmoredProcessOptions to convert.
 * @returns Equivalent ArmorSpawnOptions for armorSpawn.
 */
function toSpawnOptions(options: ArmoredProcessOptions): ArmorSpawnOptions {
  return {
    stdio: 'pipe',
    ...(options.armor !== undefined ? { armor: options.armor } : {}),
    ...(options.profile !== undefined ? { profile: options.profile } : {}),
    ...(options.noOsSandbox !== undefined ? { noOsSandbox: options.noOsSandbox } : {}),
    ...(options.cwd !== undefined ? { cwd: options.cwd } : {}),
  };
}

/**
 * Write a JSON message followed by a newline to the process stdin.
 *
 * @param proc - The child process whose stdin to write to.
 * @param message - The message object to serialize.
 * @returns A promise that resolves when the write is flushed.
 * @throws {ArmoredProcessError} If stdin is not available or the write fails.
 */
function writeMessage(proc: ChildProcess, message: Record<string, unknown>): Promise<void> {
  return new Promise((resolve, reject) => {
    if (proc.stdin === null) {
      reject(new ArmoredProcessError('Process stdin is not available'));
      return;
    }
    const line = JSON.stringify(message) + '\n';
    const flushed = proc.stdin.write(line, (err) => {
      if (err !== undefined && err !== null) {
        reject(new ArmoredProcessError('Failed to write to process stdin', { cause: err }));
      }
    });
    if (flushed) {
      resolve();
    } else {
      proc.stdin.once('drain', resolve);
    }
  });
}

/**
 * Read exactly one newline-terminated line from stdout and parse it as JSON.
 *
 * @param proc - The child process to read from.
 * @param signal - AbortSignal used to cancel the read on timeout.
 * @returns Parsed JSON object from the response line.
 * @throws {ArmoredProcessError} If stdout is unavailable, the process exits
 *   before responding, the response is not valid JSON, or the signal fires.
 */
function readResponse(
  proc: ChildProcess,
  signal: AbortSignal,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    if (proc.stdout === null) {
      reject(new ArmoredProcessError('Process stdout is not available'));
      return;
    }

    let buffer = '';

    const onData = (chunk: Buffer | string): void => {
      buffer += chunk.toString();
      const newlineIndex = buffer.indexOf('\n');
      if (newlineIndex === -1) {
        return;
      }

      const line = buffer.slice(0, newlineIndex);
      cleanup();
      parseLine(line, resolve, reject);
    };

    const onClose = (): void => {
      cleanup();
      reject(new ArmoredProcessError('Process exited before sending a response'));
    };

    const onAbort = (): void => {
      cleanup();
      reject(new ArmoredProcessError('invoke() timed out waiting for a response'));
    };

    const cleanup = (): void => {
      proc.stdout?.removeListener('data', onData);
      proc.stdout?.removeListener('close', onClose);
      signal.removeEventListener('abort', onAbort);
    };

    proc.stdout.on('data', onData);
    proc.stdout.once('close', onClose);
    signal.addEventListener('abort', onAbort, { once: true });
  });
}

/**
 * Parse a JSON line and call the appropriate resolver or rejecter.
 *
 * @param line - The raw JSON string to parse.
 * @param resolve - Promise resolve callback.
 * @param reject - Promise reject callback.
 */
function parseLine(
  line: string,
  resolve: (value: Record<string, unknown>) => void,
  reject: (reason: ArmoredProcessError) => void,
): void {
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch (err) {
    reject(new ArmoredProcessError(`Process returned invalid JSON: ${line}`, { cause: err }));
    return;
  }

  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    reject(new ArmoredProcessError(`Process returned a non-object JSON value: ${line}`));
    return;
  }

  resolve(parsed as Record<string, unknown>);
}

/**
 * Create an AbortController that fires after timeoutMs milliseconds.
 *
 * The timer is deliberately kept referenced so it fires even when other
 * event-loop activity is absent. Callers must call controller.abort() in
 * a finally block to clear the timer promptly when the operation completes.
 *
 * @param timeoutMs - Duration in milliseconds before the signal fires.
 * @returns The AbortController; call .abort() to cancel before the timeout.
 */
function createTimeoutController(timeoutMs: number): AbortController {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), timeoutMs);
  return controller;
}

// ---------------------------------------------------------------------------
// ArmoredProcess class
// ---------------------------------------------------------------------------

/**
 * Manages the lifecycle of an MCP tool subprocess running under MCP Armor.
 *
 * Supports two usage patterns:
 *
 * **Lazy spawn** (one-shot): each {@link invoke} call spawns a fresh process,
 * sends one message, reads one response, and exits.
 *
 * **Persistent spawn** (factory): use {@link ArmoredProcess.spawn} to eagerly
 * start the process. Subsequent {@link invoke} calls reuse the running process.
 * Call {@link close} when done.
 *
 * @example
 * ```typescript
 * // One-shot
 * const proc = new ArmoredProcess({ command: ['node', 'tool.js'], armor: './armor.json' });
 * const result = await proc.invoke({ method: 'list_repos', params: {} });
 *
 * // Persistent
 * const proc = await ArmoredProcess.spawn({ command: ['node', 'tool.js'], armor: './armor.json' });
 * const r1 = await proc.invoke(params1);
 * const r2 = await proc.invoke(params2);
 * await proc.close();
 * ```
 */
export class ArmoredProcess {
  readonly #options: ArmoredProcessOptions;
  #process: ChildProcess | null = null;

  /**
   * Create a new ArmoredProcess.
   *
   * The underlying subprocess is not started until the first {@link invoke}
   * call unless {@link ArmoredProcess.spawn} was used.
   *
   * @param options - Configuration for the armored process.
   */
  constructor(options: ArmoredProcessOptions) {
    this.#options = options;
  }

  /**
   * Factory that eagerly spawns the subprocess before the first invoke.
   *
   * Use this when you want to amortize startup latency across multiple invocations.
   *
   * @param options - Configuration for the armored process.
   * @returns A promise that resolves to a running {@link ArmoredProcess}.
   * @throws {ArmoredProcessError} If the process fails to start.
   */
  static async spawn(options: ArmoredProcessOptions): Promise<ArmoredProcess> {
    const instance = new ArmoredProcess(options);
    instance.#startProcess();
    return instance;
  }

  /**
   * Start the underlying subprocess if it is not already running.
   *
   * @throws {ArmoredProcessError} If the spawn fails.
   */
  #startProcess(): void {
    if (this.#process !== null) {
      return;
    }
    try {
      this.#process = armorSpawn(this.#options.command, toSpawnOptions(this.#options));
    } catch (err) {
      throw new ArmoredProcessError('Failed to spawn armored process', { cause: err });
    }
  }

  /**
   * Send a JSON-RPC message to the tool and return the parsed response.
   *
   * Writes `JSON.stringify(message) + "\n"` to stdin and reads one
   * newline-terminated line from stdout. The subprocess is started lazily
   * on the first call if not already running.
   *
   * @param message - The JSON-RPC message object to send.
   * @param options - Optional per-call settings (e.g. timeout).
   * @returns The parsed JSON response object from the tool.
   * @throws {ArmoredProcessError} On spawn failure, I/O error, invalid JSON
   *   response, or timeout.
   */
  async invoke(
    message: Record<string, unknown>,
    options: InvokeOptions = {},
  ): Promise<Record<string, unknown>> {
    this.#startProcess();

    const proc = this.#process as ChildProcess;
    const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const controller = createTimeoutController(timeoutMs);

    try {
      await writeMessage(proc, message);
      return await readResponse(proc, controller.signal);
    } finally {
      controller.abort();
    }
  }

  /**
   * Terminate the subprocess and wait for it to exit.
   *
   * Safe to call when the process is not running.
   *
   * @returns A promise that resolves when the process has exited.
   */
  close(): Promise<void> {
    const proc = this.#process;
    if (proc === null) {
      return Promise.resolve();
    }

    this.#process = null;

    return new Promise((resolve) => {
      proc.once('close', resolve);
      proc.kill();
    });
  }
}
