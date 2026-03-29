import { describe, it, mock } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';

// ---------------------------------------------------------------------------
// Fake ChildProcess (reused from process.test.ts pattern)
// ---------------------------------------------------------------------------

interface FakeStdin {
  written: string;
  write(data: string, callback?: (err?: Error | null) => void): boolean;
}

interface FakeProcess extends EventEmitter {
  pid: number;
  stdin: FakeStdin;
  stdout: EventEmitter & { push(chunk: string): void };
  kill(): void;
}

let fakeProcessCounter = 0;

function makeFakeProcess(): FakeProcess {
  const proc = new EventEmitter() as FakeProcess;
  proc.pid = ++fakeProcessCounter;

  const stdinEmitter = new EventEmitter() as EventEmitter & FakeStdin;
  stdinEmitter.written = '';
  stdinEmitter.write = (data: string, callback?: (err?: Error | null) => void): boolean => {
    stdinEmitter.written += data;
    if (callback !== undefined) {
      callback(null);
    }
    return true;
  };
  proc.stdin = stdinEmitter as FakeStdin;

  const stdoutEmitter = new EventEmitter() as EventEmitter & { push(chunk: string): void };
  stdoutEmitter.push = (chunk: string): void => {
    stdoutEmitter.emit('data', Buffer.from(chunk));
  };
  proc.stdout = stdoutEmitter;

  proc.kill = (): void => {
    proc.emit('exit', 0);
    proc.emit('close', 0);
  };

  return proc;
}

// ---------------------------------------------------------------------------
// Module loading with mocked spawn
// ---------------------------------------------------------------------------

async function loadPoolWithMock(): Promise<{
  ArmoredPool: typeof import('../src/pool.ts').ArmoredPool;
  ArmoredPoolError: typeof import('../src/pool.ts').ArmoredPoolError;
}> {
  mock.reset();

  await mock.module('../src/spawn.ts', {
    namedExports: {
      armorSpawn: () => makeFakeProcess(),
    },
  });

  const mod = await import('../src/pool.ts?nocache=' + Math.random());
  return { ArmoredPool: mod.ArmoredPool, ArmoredPoolError: mod.ArmoredPoolError };
}

// ---------------------------------------------------------------------------
// Constructor validation
// ---------------------------------------------------------------------------

describe('ArmoredPool — constructor', () => {
  it('throws RangeError when size is 0', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    assert.throws(
      () => new ArmoredPool({ command: ['node', 'tool.js'], size: 0 }),
      RangeError,
    );
  });

  it('throws RangeError when size is negative', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    assert.throws(
      () => new ArmoredPool({ command: ['node', 'tool.js'], size: -3 }),
      RangeError,
    );
  });

  it('defaults to size 4', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'] });
    assert.equal(pool.size, 4);
  });
});

// ---------------------------------------------------------------------------
// start()
// ---------------------------------------------------------------------------

describe('ArmoredPool.start', () => {
  it('spawns the configured number of processes', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 3 });
    await pool.start();
    assert.equal(pool.available, 3);
    await pool.close();
  });

  it('throws if called twice', async () => {
    const { ArmoredPool, ArmoredPoolError } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 1 });
    await pool.start();
    await assert.rejects(pool.start(), ArmoredPoolError);
    await pool.close();
  });

  it('throws if pool was closed', async () => {
    const { ArmoredPool, ArmoredPoolError } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 1 });
    await pool.start();
    await pool.close();
    await assert.rejects(pool.start(), ArmoredPoolError);
  });
});

// ---------------------------------------------------------------------------
// acquire() / release()
// ---------------------------------------------------------------------------

describe('ArmoredPool.acquire', () => {
  it('returns a process and decrements available count', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 2 });
    await pool.start();

    const proc = await pool.acquire();
    assert.equal(pool.available, 1);
    assert.ok(proc !== null);

    pool.release(proc);
    assert.equal(pool.available, 2);
    await pool.close();
  });

  it('throws before start()', async () => {
    const { ArmoredPool, ArmoredPoolError } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 1 });
    await assert.rejects(pool.acquire(), ArmoredPoolError);
  });

  it('throws after close()', async () => {
    const { ArmoredPool, ArmoredPoolError } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 1 });
    await pool.start();
    await pool.close();
    await assert.rejects(pool.acquire(), ArmoredPoolError);
  });
});

// ---------------------------------------------------------------------------
// close()
// ---------------------------------------------------------------------------

describe('ArmoredPool.close', () => {
  it('is idempotent', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 1 });
    await pool.start();
    await pool.close();
    await assert.doesNotReject(pool.close());
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe('ArmoredPool — edge cases', () => {
  it('size property reflects configuration', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 7 });
    assert.equal(pool.size, 7);
  });

  it('available is 0 before start', async () => {
    const { ArmoredPool } = await loadPoolWithMock();
    const pool = new ArmoredPool({ command: ['node', 'tool.js'], size: 3 });
    assert.equal(pool.available, 0);
  });
});
