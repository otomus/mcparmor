import { describe, it, mock } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { type Writable, type Readable } from 'node:stream';

// ---------------------------------------------------------------------------
// Fake ChildProcess
//
// Emulates the ChildProcess interface used by ArmoredProcess:
//   - stdin: Writable (captures written data)
//   - stdout: Readable (we push data into it)
//   - kill(): triggers 'close' event
//   - once('close', fn): standard EventEmitter
// ---------------------------------------------------------------------------

interface FakeStdin {
  written: string;
  write(data: string, callback?: (err?: Error | null) => void): boolean;
}

interface FakeProcess extends EventEmitter {
  stdin: FakeStdin;
  stdout: EventEmitter & { push(chunk: string): void };
  kill(): void;
}

function makeFakeProcess(): FakeProcess {
  const proc = new EventEmitter() as FakeProcess;

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
    proc.emit('close', 0);
  };

  return proc;
}

// ---------------------------------------------------------------------------
// Module-level mock strategy
//
// We mock '../src/spawn.ts' so ArmoredProcess never spawns real processes.
// Each test gets its own fake process instance via a closure.
// ---------------------------------------------------------------------------

let currentFakeProcess: FakeProcess | null = null;

async function loadProcessWithMock(fakeProcess?: FakeProcess): Promise<{
  ArmoredProcess: typeof import('../src/process.ts').ArmoredProcess;
  ArmoredProcessError: typeof import('../src/process.ts').ArmoredProcessError;
  fake: FakeProcess;
}> {
  const fake = fakeProcess ?? makeFakeProcess();
  currentFakeProcess = fake;
  mock.reset();

  await mock.module('../src/spawn.ts', {
    namedExports: {
      armorSpawn: () => fake,
    },
  });

  const mod = await import('../src/process.ts?nocache=' + Math.random());
  return { ArmoredProcess: mod.ArmoredProcess, ArmoredProcessError: mod.ArmoredProcessError, fake };
}

// ---------------------------------------------------------------------------
// invoke() — basic send and receive
// ---------------------------------------------------------------------------

describe('ArmoredProcess.invoke — send and receive', () => {
  it('writes the message as a JSON line to stdin', async () => {
    const { ArmoredProcess, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    const message = { method: 'list_repos', params: {} };
    const responsePayload = { result: ['repo1', 'repo2'] };

    // Respond after a tick so invoke() has time to register the stdout listener.
    setImmediate(() => {
      fake.stdout.push(JSON.stringify(responsePayload) + '\n');
    });

    await proc.invoke(message);

    assert.equal(fake.stdin.written, JSON.stringify(message) + '\n');
  });

  it('returns the parsed JSON response from stdout', async () => {
    const { ArmoredProcess, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    const responsePayload = { id: 1, result: { ok: true } };

    setImmediate(() => {
      fake.stdout.push(JSON.stringify(responsePayload) + '\n');
    });

    const result = await proc.invoke({ method: 'ping', params: {} });

    assert.deepEqual(result, responsePayload);
  });

  it('handles a response with nested objects', async () => {
    const { ArmoredProcess, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    const responsePayload = { result: { repos: [{ name: 'foo', stars: 99 }] } };

    setImmediate(() => {
      fake.stdout.push(JSON.stringify(responsePayload) + '\n');
    });

    const result = await proc.invoke({ method: 'list', params: {} });
    assert.deepEqual(result, responsePayload);
  });

  it('handles multiple sequential invocations reusing the same process', async () => {
    const { ArmoredProcess, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    // First invocation
    setImmediate(() => fake.stdout.push(JSON.stringify({ id: 1 }) + '\n'));
    const r1 = await proc.invoke({ method: 'op1' });
    assert.equal((r1 as { id: number }).id, 1);

    // Second invocation
    setImmediate(() => fake.stdout.push(JSON.stringify({ id: 2 }) + '\n'));
    const r2 = await proc.invoke({ method: 'op2' });
    assert.equal((r2 as { id: number }).id, 2);
  });
});

// ---------------------------------------------------------------------------
// ArmoredProcess.spawn — factory
// ---------------------------------------------------------------------------

describe('ArmoredProcess.spawn — factory', () => {
  it('returns an ArmoredProcess instance', async () => {
    const { ArmoredProcess } = await loadProcessWithMock();
    const proc = await ArmoredProcess.spawn({ command: ['node', 'tool.js'] });
    assert.ok(proc instanceof ArmoredProcess);
    await proc.close();
  });

  it('invoke() works on a factory-spawned instance', async () => {
    const { ArmoredProcess, fake } = await loadProcessWithMock();
    const proc = await ArmoredProcess.spawn({ command: ['node', 'tool.js'] });

    const payload = { status: 'ok' };
    setImmediate(() => fake.stdout.push(JSON.stringify(payload) + '\n'));

    const result = await proc.invoke({ method: 'check' });
    assert.deepEqual(result, payload);
    await proc.close();
  });
});

// ---------------------------------------------------------------------------
// close()
// ---------------------------------------------------------------------------

describe('ArmoredProcess.close', () => {
  it('resolves without error when the process is running', async () => {
    const { ArmoredProcess } = await loadProcessWithMock();
    const proc = await ArmoredProcess.spawn({ command: ['node', 'tool.js'] });
    await assert.doesNotReject(proc.close());
  });

  it('resolves without error when called on a never-started process', async () => {
    const { ArmoredProcess } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });
    await assert.doesNotReject(proc.close());
  });

  it('is idempotent — calling close twice does not throw', async () => {
    const { ArmoredProcess } = await loadProcessWithMock();
    const proc = await ArmoredProcess.spawn({ command: ['node', 'tool.js'] });
    await proc.close();
    await assert.doesNotReject(proc.close());
  });
});

// ---------------------------------------------------------------------------
// invoke() — timeout
// ---------------------------------------------------------------------------

describe('ArmoredProcess.invoke — timeout', () => {
  it('rejects with ArmoredProcessError when the timeout expires', async () => {
    const { ArmoredProcess, ArmoredProcessError } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    // Never respond — let the timeout fire.
    await assert.rejects(
      proc.invoke({ method: 'slow' }, { timeoutMs: 50 }),
      ArmoredProcessError,
    );
  });

  it('ArmoredProcessError on timeout has a descriptive message', async () => {
    const { ArmoredProcess, ArmoredProcessError } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    try {
      await proc.invoke({ method: 'slow' }, { timeoutMs: 30 });
      assert.fail('expected rejection');
    } catch (err) {
      assert.ok(err instanceof ArmoredProcessError);
      assert.ok(
        err.message.toLowerCase().includes('timeout') ||
          err.message.toLowerCase().includes('timed out'),
        `expected timeout-related message, got: ${err.message}`,
      );
    }
  });
});

// ---------------------------------------------------------------------------
// ArmoredProcessError — spawn failure
// ---------------------------------------------------------------------------

describe('ArmoredProcess — spawn failure', () => {
  it('throws ArmoredProcessError when armorSpawn throws', async () => {
    mock.reset();
    await mock.module('../src/spawn.ts', {
      namedExports: {
        armorSpawn: () => {
          throw new Error('binary not found');
        },
      },
    });

    const { ArmoredProcess, ArmoredProcessError } = await import(
      '../src/process.ts?nocache=' + Math.random()
    );

    // armorSpawn throws synchronously inside the async invoke() → becomes a rejection
    await assert.rejects(
      async () => {
        const proc2 = new ArmoredProcess({ command: ['node', 'tool.js'] });
        return proc2.invoke({ method: 'test' });
      },
      ArmoredProcessError,
    );
  });
});

// ---------------------------------------------------------------------------
// Edge cases — invalid JSON response
// ---------------------------------------------------------------------------

describe('ArmoredProcess.invoke — invalid JSON response', () => {
  it('rejects with ArmoredProcessError when stdout emits non-JSON text', async () => {
    const { ArmoredProcess, ArmoredProcessError, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    setImmediate(() => fake.stdout.push('not json at all\n'));

    await assert.rejects(proc.invoke({ method: 'test' }), ArmoredProcessError);
  });

  it('rejects with ArmoredProcessError when stdout emits a JSON array (not an object)', async () => {
    const { ArmoredProcess, ArmoredProcessError, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    setImmediate(() => fake.stdout.push(JSON.stringify([1, 2, 3]) + '\n'));

    await assert.rejects(proc.invoke({ method: 'test' }), ArmoredProcessError);
  });

  it('rejects with ArmoredProcessError when stdout emits a JSON string (not an object)', async () => {
    const { ArmoredProcess, ArmoredProcessError, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    setImmediate(() => fake.stdout.push(JSON.stringify('just a string') + '\n'));

    await assert.rejects(proc.invoke({ method: 'test' }), ArmoredProcessError);
  });

  it('rejects with ArmoredProcessError when stdout emits a JSON null', async () => {
    const { ArmoredProcess, ArmoredProcessError, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    setImmediate(() => fake.stdout.push('null\n'));

    await assert.rejects(proc.invoke({ method: 'test' }), ArmoredProcessError);
  });
});

// ---------------------------------------------------------------------------
// Edge cases — process exits before response
// ---------------------------------------------------------------------------

describe('ArmoredProcess.invoke — process exits before response', () => {
  it('rejects with ArmoredProcessError when the process closes without sending a response', async () => {
    const { ArmoredProcess, ArmoredProcessError, fake } = await loadProcessWithMock();
    const proc = new ArmoredProcess({ command: ['node', 'tool.js'] });

    setImmediate(() => fake.stdout.emit('close'));

    await assert.rejects(proc.invoke({ method: 'test' }), ArmoredProcessError);
  });
});

// ---------------------------------------------------------------------------
// ArmoredProcessError shape
// ---------------------------------------------------------------------------

describe('ArmoredProcessError', () => {
  it('has name "ArmoredProcessError"', async () => {
    const { ArmoredProcessError } = await loadProcessWithMock();
    const err = new ArmoredProcessError('test');
    assert.equal(err.name, 'ArmoredProcessError');
  });

  it('is an instance of Error', async () => {
    const { ArmoredProcessError } = await loadProcessWithMock();
    assert.ok(new ArmoredProcessError('test') instanceof Error);
  });

  it('accepts a cause option', async () => {
    const { ArmoredProcessError } = await loadProcessWithMock();
    const cause = new Error('root cause');
    const err = new ArmoredProcessError('outer', { cause });
    assert.equal((err as NodeJS.ErrnoException).cause, cause);
  });
});
