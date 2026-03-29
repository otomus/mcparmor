/**
 * ArmoredPool — pre-spawned pool of ArmoredProcess instances.
 *
 * Manages a fixed-size collection of warm processes for workloads that
 * maintain many concurrent tool subprocesses (e.g. Arqitect's 50-process pool).
 */

import { ArmoredProcess, ArmoredProcessError, type ArmoredProcessOptions } from './process.ts';

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/** Thrown on pool-level errors (start, acquire, release, close). */
export class ArmoredPoolError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'ArmoredPoolError';
  }
}

// ---------------------------------------------------------------------------
// Pool options
// ---------------------------------------------------------------------------

/** Configuration for {@link ArmoredPool}. */
export interface ArmoredPoolOptions {
  /** The tool command and arguments for each process. */
  readonly command: readonly string[];
  /** Path to the armor.json capability manifest. */
  readonly armor?: string;
  /** Number of processes to pre-spawn. Default: 4. */
  readonly size?: number;
  /**
   * If true, each process waits for a `{"ready": true}` JSON line
   * from stdout before becoming available.
   */
  readonly readySignal?: boolean;
  /** Override the base profile declared in armor.json. */
  readonly profile?: string;
  /** Disable OS-level sandbox (skips Layer 2 enforcement). */
  readonly noOsSandbox?: boolean;
  /** Working directory for spawned processes. */
  readonly cwd?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_POOL_SIZE = 4;
const MIN_POOL_SIZE = 1;

// ---------------------------------------------------------------------------
// ArmoredPool class
// ---------------------------------------------------------------------------

/**
 * A fixed-size pool of warm {@link ArmoredProcess} instances.
 *
 * Pre-spawns *size* processes at {@link start} time and hands them out via
 * {@link acquire} / {@link release}.
 *
 * @example
 * ```typescript
 * const pool = new ArmoredPool({
 *   command: ['node', 'tool.js'],
 *   armor: './armor.json',
 *   size: 10,
 * });
 * await pool.start();
 *
 * const proc = await pool.acquire();
 * try {
 *   const result = await proc.invoke({ method: 'run', params: {} });
 * } finally {
 *   pool.release(proc);
 * }
 *
 * await pool.close();
 * ```
 */
export class ArmoredPool {
  readonly #options: ArmoredPoolOptions;
  readonly #size: number;
  readonly #available: ArmoredProcess[] = [];
  readonly #all: ArmoredProcess[] = [];
  readonly #waiters: Array<(proc: ArmoredProcess) => void> = [];
  #started = false;
  #closed = false;

  /**
   * Create a new ArmoredPool.
   *
   * @param options - Pool configuration.
   * @throws {RangeError} If size is less than 1.
   */
  constructor(options: ArmoredPoolOptions) {
    const size = options.size ?? DEFAULT_POOL_SIZE;
    if (size < MIN_POOL_SIZE) {
      throw new RangeError('Pool size must be at least 1');
    }
    this.#options = options;
    this.#size = size;
  }

  // ------------------------------------------------------------------
  // Public API
  // ------------------------------------------------------------------

  /**
   * Pre-spawn all processes in the pool.
   *
   * If `readySignal` was set, each process waits for the ready handshake
   * before joining the available queue.
   *
   * @throws {ArmoredPoolError} If the pool is already started or closed.
   */
  async start(): Promise<void> {
    if (this.#started) {
      throw new ArmoredPoolError('Pool is already started');
    }
    if (this.#closed) {
      throw new ArmoredPoolError('Pool has been closed');
    }

    const spawnPromises: Array<Promise<ArmoredProcess>> = [];
    for (let i = 0; i < this.#size; i++) {
      spawnPromises.push(this.#createProcess());
    }

    const processes = await Promise.all(spawnPromises);
    for (const proc of processes) {
      this.#all.push(proc);
      this.#available.push(proc);
    }

    this.#started = true;
  }

  /**
   * Acquire an available process from the pool.
   *
   * If no process is currently available, the returned promise blocks
   * until one is released via {@link release}.
   *
   * @returns A running {@link ArmoredProcess}.
   * @throws {ArmoredPoolError} If the pool is not started or has been closed.
   */
  async acquire(): Promise<ArmoredProcess> {
    if (!this.#started) {
      throw new ArmoredPoolError('Pool is not started. Call start() first');
    }
    if (this.#closed) {
      throw new ArmoredPoolError('Pool has been closed');
    }

    const proc = this.#available.shift();
    if (proc !== undefined) {
      return proc;
    }

    return new Promise<ArmoredProcess>((resolve) => {
      this.#waiters.push(resolve);
    });
  }

  /**
   * Return a process to the pool.
   *
   * If the process has died, it is replaced with a freshly spawned one.
   *
   * @param proc - The process previously obtained via {@link acquire}.
   */
  release(proc: ArmoredProcess): void {
    if (this.#closed) {
      void proc.close();
      return;
    }

    if (!proc.isAlive()) {
      void proc.close();
      void this.#replaceProcess(proc);
      return;
    }

    this.#handOff(proc);
  }

  /**
   * Close all processes in the pool.
   *
   * Safe to call multiple times; subsequent calls are no-ops.
   */
  async close(): Promise<void> {
    if (this.#closed) {
      return;
    }
    this.#closed = true;

    const closePromises = this.#all.map((proc) => proc.close());
    await Promise.all(closePromises);
    this.#all.length = 0;
    this.#available.length = 0;
  }

  /** The configured pool size. */
  get size(): number {
    return this.#size;
  }

  /** The number of currently available processes. */
  get available(): number {
    return this.#available.length;
  }

  // ------------------------------------------------------------------
  // Private helpers
  // ------------------------------------------------------------------

  /**
   * Create and spawn a single ArmoredProcess.
   *
   * @returns A running process, optionally ready-signalled.
   */
  async #createProcess(): Promise<ArmoredProcess> {
    const processOptions: ArmoredProcessOptions = {
      command: this.#options.command,
      ...(this.#options.armor !== undefined ? { armor: this.#options.armor } : {}),
      ...(this.#options.profile !== undefined ? { profile: this.#options.profile } : {}),
      ...(this.#options.noOsSandbox !== undefined ? { noOsSandbox: this.#options.noOsSandbox } : {}),
      ...(this.#options.cwd !== undefined ? { cwd: this.#options.cwd } : {}),
      ...(this.#options.readySignal !== undefined ? { readySignal: this.#options.readySignal } : {}),
    };
    return ArmoredProcess.spawn(processOptions);
  }

  /**
   * Replace a dead process in the pool with a fresh one.
   *
   * @param dead - The process that died.
   */
  async #replaceProcess(dead: ArmoredProcess): Promise<void> {
    const replacement = await this.#createProcess();
    const index = this.#all.indexOf(dead);
    if (index !== -1) {
      this.#all[index] = replacement;
    } else {
      this.#all.push(replacement);
    }
    this.#handOff(replacement);
  }

  /**
   * Hand a process to a waiting acquirer, or put it in the available queue.
   *
   * @param proc - The process to hand off.
   */
  #handOff(proc: ArmoredProcess): void {
    const waiter = this.#waiters.shift();
    if (waiter !== undefined) {
      waiter(proc);
    } else {
      this.#available.push(proc);
    }
  }
}
