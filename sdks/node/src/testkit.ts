/**
 * MCP Armor TestKit — test your armor.json policies against the real broker.
 *
 * Provides {@link ArmorTestHarness}, which spins up the real mcparmor broker
 * backed by a lightweight mock MCP tool server. You define what responses the
 * mock tool returns; the harness sends `tools/call` messages through the
 * broker and reports whether they were blocked, allowed, or had secrets
 * redacted.
 *
 * @example
 * ```typescript
 * const harness = await ArmorTestHarness.start({ armor: './armor.json' });
 * harness.mockToolResponse({ content: [{ type: 'text', text: 'hello' }] });
 * const result = await harness.callTool('read_file', { path: '/etc/passwd' });
 * assert(result.blocked);
 * await harness.stop();
 * ```
 */

import { spawn, type ChildProcess } from 'node:child_process';
import { writeFileSync, mkdtempSync, rmSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { findBinary } from './binary.ts';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** JSON-RPC error codes returned by the mcparmor broker on policy violations. */
export const ArmorErrorCode = {
  PATH_VIOLATION: -32001,
  NETWORK_VIOLATION: -32002,
  SPAWN_VIOLATION: -32003,
  SECRET_BLOCKED: -32004,
  TIMEOUT: -32005,
} as const;

/** The outcome of sending a `tools/call` message through the broker. */
export interface ToolCallResult {
  /** The full JSON-RPC response envelope. */
  readonly raw: Record<string, unknown>;
  /** True if the broker returned an error (policy violation). */
  readonly blocked: boolean;
  /** True if the call passed through to the mock tool. */
  readonly allowed: boolean;
  /** The JSON-RPC error code, or null if not blocked. */
  readonly errorCode: number | null;
  /** The error message string, or null if not blocked. */
  readonly errorMessage: string | null;
  /** The `result` payload from the mock tool, or null if blocked. */
  readonly response: Record<string, unknown> | null;
  /** The first text content from the tool response, or null. */
  readonly text: string | null;
}

/** Options for creating an {@link ArmorTestHarness}. */
export interface ArmorTestHarnessOptions {
  /** Path to the armor.json manifest to test. */
  readonly armor: string;
  /** Optional profile override (e.g. `"strict"`). */
  readonly profile?: string;
  /**
   * Disable Layer 2 OS sandbox. Defaults to `true` because the testkit tests
   * Layer 1 enforcement; the OS sandbox would interfere with the mock server.
   */
  readonly noOsSandbox?: boolean;
  /** Read timeout in milliseconds for individual tool calls. Default: 10 000. */
  readonly timeoutMs?: number;
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/** Thrown when the test harness cannot be started or communication fails. */
export class ArmorTestHarnessError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'ArmorTestHarnessError';
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

const DEFAULT_TIMEOUT_MS = 10_000;

/** Resolve the path to the bundled mock-tool-server.js. */
function resolveMockServerPath(): string {
  const currentDir = dirname(fileURLToPath(import.meta.url));
  return join(currentDir, 'mock-tool-server.js');
}

/**
 * Read exactly one newline-terminated line from the process stdout.
 *
 * @param proc - The child process to read from.
 * @param timeoutMs - Maximum milliseconds to wait.
 * @returns Parsed JSON response object.
 */
function readLine(
  proc: ChildProcess,
  timeoutMs: number,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    if (proc.stdout === null) {
      reject(new ArmorTestHarnessError('Process stdout is not available'));
      return;
    }

    let buffer = '';
    let settled = false;

    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new ArmorTestHarnessError('Timed out waiting for broker response'));
    }, timeoutMs);

    const onData = (chunk: Buffer | string): void => {
      if (settled) return;
      buffer += chunk.toString();
      const idx = buffer.indexOf('\n');
      if (idx === -1) return;

      const line = buffer.slice(0, idx);
      settled = true;
      cleanup();

      try {
        const parsed = JSON.parse(line) as Record<string, unknown>;
        resolve(parsed);
      } catch (err) {
        reject(new ArmorTestHarnessError(`Broker returned invalid JSON: ${line}`, { cause: err }));
      }
    };

    const onClose = (): void => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new ArmorTestHarnessError('Broker closed stdout without responding'));
    };

    const cleanup = (): void => {
      clearTimeout(timer);
      proc.stdout?.removeListener('data', onData);
      proc.stdout?.removeListener('close', onClose);
    };

    proc.stdout.on('data', onData);
    proc.stdout.once('close', onClose);
  });
}

/**
 * Write a JSON-RPC message to the process stdin.
 *
 * @param proc - The child process whose stdin to write to.
 * @param message - The message to serialize and send.
 */
function writeMessage(proc: ChildProcess, message: Record<string, unknown>): Promise<void> {
  return new Promise((resolve, reject) => {
    if (proc.stdin === null) {
      reject(new ArmorTestHarnessError('Process stdin is not available'));
      return;
    }
    const line = JSON.stringify(message) + '\n';
    const onDrain = (): void => resolve();
    const flushed = proc.stdin.write(line, (err) => {
      if (err != null) {
        proc.stdin?.removeListener('drain', onDrain);
        reject(new ArmorTestHarnessError('Failed to write to broker stdin', { cause: err }));
      }
    });
    if (flushed) {
      resolve();
    } else {
      proc.stdin.once('drain', onDrain);
    }
  });
}

/**
 * Classify a JSON-RPC response as blocked or allowed.
 *
 * @param raw - The full JSON-RPC response envelope.
 * @returns A populated {@link ToolCallResult}.
 */
function classifyResponse(raw: Record<string, unknown>): ToolCallResult {
  const error = raw['error'] as Record<string, unknown> | undefined;
  if (error !== undefined) {
    return {
      raw,
      blocked: true,
      allowed: false,
      errorCode: (error['code'] as number) ?? null,
      errorMessage: (error['message'] as string) ?? null,
      response: null,
      text: null,
    };
  }

  const response = (raw['result'] as Record<string, unknown>) ?? null;
  let text: string | null = null;
  if (response !== null) {
    const content = response['content'] as Array<Record<string, unknown>> | undefined;
    if (content !== undefined && content.length > 0) {
      text = (content[0]?.['text'] as string) ?? null;
    }
  }

  return {
    raw,
    blocked: false,
    allowed: true,
    errorCode: null,
    errorMessage: null,
    response,
    text,
  };
}

// ---------------------------------------------------------------------------
// ArmorTestHarness
// ---------------------------------------------------------------------------

/**
 * Test harness that runs the real mcparmor broker with a mock tool behind it.
 *
 * Use the static {@link ArmorTestHarness.start} factory to create and
 * initialise an instance. Call {@link ArmorTestHarness.stop} when done.
 */
export class ArmorTestHarness {
  readonly #armor: string;
  readonly #profile: string | undefined;
  readonly #noOsSandbox: boolean;
  readonly #timeoutMs: number;
  #process: ChildProcess | null = null;
  #tmpDir: string | null = null;
  #configPath: string = '';
  #idCounter: number = 0;
  #config: Record<string, unknown>;

  private constructor(options: ArmorTestHarnessOptions) {
    this.#armor = options.armor;
    this.#profile = options.profile;
    this.#noOsSandbox = options.noOsSandbox ?? true;
    this.#timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.#config = {
      server_info: { name: 'mcparmor-mock-tool', version: '1.0' },
      tools: [],
      default_response: {
        content: [{ type: 'text', text: 'mock response' }],
      },
      responses: {},
    };
  }

  /**
   * Create and start a test harness.
   *
   * Spawns the real mcparmor broker, performs the MCP handshake, and returns
   * a ready-to-use harness.
   *
   * @param options - Configuration for the test harness.
   * @returns A running {@link ArmorTestHarness}.
   */
  static async start(options: ArmorTestHarnessOptions): Promise<ArmorTestHarness> {
    const harness = new ArmorTestHarness(options);
    harness.#spawn();
    await harness.#handshake();
    return harness;
  }

  // ------------------------------------------------------------------
  // Public API
  // ------------------------------------------------------------------

  /**
   * Set the response the mock tool returns for `tools/call`.
   *
   * @param response - The MCP result payload.
   * @param toolName - If given, this response is used only for this tool name.
   *   Otherwise it becomes the default response for all tools.
   */
  mockToolResponse(response: Record<string, unknown>, toolName?: string): void {
    if (toolName !== undefined) {
      (this.#config['responses'] as Record<string, unknown>)[toolName] = response;
    } else {
      this.#config['default_response'] = response;
    }
    this.#writeConfig();
  }

  /**
   * Set the tool definitions returned by `tools/list`.
   *
   * @param tools - List of MCP tool definition objects.
   */
  setTools(tools: ReadonlyArray<Record<string, unknown>>): void {
    this.#config['tools'] = [...tools];
    this.#writeConfig();
  }

  /**
   * Send a `tools/call` JSON-RPC message through the broker.
   *
   * @param name - The tool name to call.
   * @param args - The arguments to pass to the tool.
   * @returns A {@link ToolCallResult} describing the outcome.
   */
  async callTool(
    name: string,
    args: Record<string, unknown> = {},
  ): Promise<ToolCallResult> {
    const message = {
      jsonrpc: '2.0',
      id: this.#nextId(),
      method: 'tools/call',
      params: { name, arguments: args },
    };
    const raw = await this.#sendAndReceive(message);
    return classifyResponse(raw);
  }

  /**
   * Send an arbitrary JSON-RPC message and return the raw response.
   *
   * @param message - A JSON-RPC request object.
   * @returns The parsed JSON-RPC response.
   */
  async sendRaw(message: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.#sendAndReceive(message);
  }

  /**
   * Terminate the broker and clean up resources.
   *
   * @returns A promise that resolves when the process has exited.
   */
  async stop(): Promise<void> {
    const proc = this.#process;
    if (proc !== null) {
      this.#process = null;
      await new Promise<void>((resolve) => {
        proc.once('close', () => resolve());
        proc.kill();
      });
    }
    if (this.#tmpDir !== null) {
      rmSync(this.#tmpDir, { recursive: true, force: true });
      this.#tmpDir = null;
    }
  }

  // ------------------------------------------------------------------
  // Private helpers
  // ------------------------------------------------------------------

  #nextId(): number {
    this.#idCounter += 1;
    return this.#idCounter;
  }

  /** Write the config atomically (write to .tmp then rename). */
  #writeConfig(): void {
    const tmpPath = this.#configPath + '.tmp';
    writeFileSync(tmpPath, JSON.stringify(this.#config));
    renameSync(tmpPath, this.#configPath);
  }

  #spawn(): void {
    this.#tmpDir = mkdtempSync(join(tmpdir(), 'mcparmor-testkit-'));
    this.#configPath = join(this.#tmpDir, 'mock_config.json');
    this.#writeConfig();

    const binaryPath = findBinary();
    const mockServerPath = resolveMockServerPath();

    const brokerArgs: string[] = ['run', '--armor', this.#armor];
    if (this.#profile !== undefined) {
      brokerArgs.push('--profile', this.#profile);
    }
    if (this.#noOsSandbox) {
      brokerArgs.push('--no-os-sandbox');
    }
    brokerArgs.push('--no-audit', '--', process.execPath, mockServerPath, this.#configPath);

    this.#process = spawn(binaryPath, brokerArgs, {
      stdio: 'pipe',
    });
  }

  async #handshake(): Promise<void> {
    const initMsg = {
      jsonrpc: '2.0',
      id: this.#nextId(),
      method: 'initialize',
      params: {
        protocolVersion: '2024-11-05',
        capabilities: {},
        clientInfo: { name: 'mcparmor-testkit', version: '1.0' },
      },
    };
    const response = await this.#sendAndReceive(initMsg);
    if ('error' in response) {
      throw new ArmorTestHarnessError(
        `MCP handshake failed: ${JSON.stringify(response['error'])}`,
      );
    }

    const notification = {
      jsonrpc: '2.0',
      method: 'notifications/initialized',
    };
    await writeMessage(this.#process!, notification);
  }

  async #sendAndReceive(
    message: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const proc = this.#process;
    if (proc === null) {
      throw new ArmorTestHarnessError('Broker process is not running');
    }
    await writeMessage(proc, message);
    return readLine(proc, this.#timeoutMs);
  }
}
