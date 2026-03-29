import { describe, it, mock, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { type ChildProcess } from 'node:child_process';

// ---------------------------------------------------------------------------
// Module-level mocking strategy
//
// node:test's mock.module() patches module imports. We mock two modules:
//   - '../src/binary.ts'  → controls findBinary return value / throws
//   - 'node:child_process' → captures spawn call arguments
// ---------------------------------------------------------------------------

const FAKE_BINARY = '/usr/local/bin/mcparmor';

/** Captured spawn invocations for assertion. */
interface SpawnCall {
  binary: string;
  args: string[];
  options: Record<string, unknown>;
}

let capturedSpawnCalls: SpawnCall[] = [];

/** A minimal ChildProcess stand-in returned by the mock. */
function makeFakeProcess(): Partial<ChildProcess> {
  return { pid: 1234 };
}

// ---------------------------------------------------------------------------
// We use a dynamic import inside each test block so that mock.module() takes
// effect. Node's test runner with --experimental-strip-types supports this.
// ---------------------------------------------------------------------------

async function loadSpawnWithMocks({
  binaryResult,
  binaryThrows,
}: {
  binaryResult?: string;
  binaryThrows?: Error;
}): Promise<typeof import('../src/spawn.ts')> {
  capturedSpawnCalls = [];
  mock.reset();

  await mock.module('../src/binary.ts', {
    namedExports: {
      findBinary: () => {
        if (binaryThrows !== undefined) throw binaryThrows;
        return binaryResult ?? FAKE_BINARY;
      },
    },
  });

  await mock.module('node:child_process', {
    namedExports: {
      spawn: (binary: string, args: string[], options: Record<string, unknown>) => {
        capturedSpawnCalls.push({ binary, args, options });
        return makeFakeProcess() as ChildProcess;
      },
    },
  });

  // Re-import to pick up fresh mocks
  return import('../src/spawn.ts?nocache=' + Math.random());
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

describe('armorSpawn — input validation', () => {
  it('throws TypeError for an empty command array', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    assert.throws(() => armorSpawn([]), (err: unknown) => {
      assert.ok(err instanceof TypeError);
      return true;
    });
  });

  it('throws TypeError when command is a string instead of an array', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.throws(() => (armorSpawn as any)('node tool.js'), (err: unknown) => {
      assert.ok(err instanceof TypeError);
      return true;
    });
  });

  it('throws TypeError when command is null', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.throws(() => (armorSpawn as any)(null), (err: unknown) => {
      assert.ok(err instanceof TypeError);
      return true;
    });
  });

  it('throws TypeError when command is undefined', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.throws(() => (armorSpawn as any)(undefined), (err: unknown) => {
      assert.ok(err instanceof TypeError);
      return true;
    });
  });

  it('throws TypeError when command is a number', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assert.throws(() => (armorSpawn as any)(42), (err: unknown) => {
      assert.ok(err instanceof TypeError);
      return true;
    });
  });
});

// ---------------------------------------------------------------------------
// Broker binary prepended / separator
// ---------------------------------------------------------------------------

describe('armorSpawn — broker wrapping', () => {
  it('spawns the mcparmor binary (not the tool directly)', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js']);
    assert.equal(capturedSpawnCalls.length, 1);
    assert.equal(capturedSpawnCalls[0]?.binary, FAKE_BINARY);
  });

  it('includes "run" as the first broker argument', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js']);
    assert.equal(capturedSpawnCalls[0]?.args[0], 'run');
  });

  it('places a "--" separator between broker flags and the tool command', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { armor: './armor.json' });
    const args = capturedSpawnCalls[0]?.args ?? [];
    const separatorIndex = args.indexOf('--');
    assert.ok(separatorIndex >= 0, '"--" separator must be present');
    // Everything after "--" is the tool command
    assert.deepEqual(args.slice(separatorIndex + 1), ['node', 'tool.js']);
  });

  it('passes through a multi-element tool command verbatim', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['python', '-m', 'myserver', '--port', '3000']);
    const args = capturedSpawnCalls[0]?.args ?? [];
    const sep = args.indexOf('--');
    assert.deepEqual(args.slice(sep + 1), ['python', '-m', 'myserver', '--port', '3000']);
  });
});

// ---------------------------------------------------------------------------
// Armor flags
// ---------------------------------------------------------------------------

describe('armorSpawn — armor flags', () => {
  it('includes --armor flag when armor option is given', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { armor: './armor.json' });
    const args = capturedSpawnCalls[0]?.args ?? [];
    const armorIdx = args.indexOf('--armor');
    assert.ok(armorIdx >= 0, '--armor flag must be present');
    assert.equal(args[armorIdx + 1], './armor.json');
  });

  it('omits --armor flag when armor option is not given', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js']);
    const args = capturedSpawnCalls[0]?.args ?? [];
    assert.ok(!args.includes('--armor'), '--armor flag must be absent');
  });

  it('includes --profile flag when profile option is given', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { profile: 'strict' });
    const args = capturedSpawnCalls[0]?.args ?? [];
    const profileIdx = args.indexOf('--profile');
    assert.ok(profileIdx >= 0, '--profile flag must be present');
    assert.equal(args[profileIdx + 1], 'strict');
  });

  it('omits --profile flag when profile option is not given', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js']);
    assert.ok(!(capturedSpawnCalls[0]?.args ?? []).includes('--profile'));
  });

  it('includes --no-os-sandbox flag when noOsSandbox is true', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { noOsSandbox: true });
    assert.ok((capturedSpawnCalls[0]?.args ?? []).includes('--no-os-sandbox'));
  });

  it('omits --no-os-sandbox flag when noOsSandbox is false', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { noOsSandbox: false });
    assert.ok(!(capturedSpawnCalls[0]?.args ?? []).includes('--no-os-sandbox'));
  });

  it('omits --no-os-sandbox flag when noOsSandbox is not provided', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js']);
    assert.ok(!(capturedSpawnCalls[0]?.args ?? []).includes('--no-os-sandbox'));
  });

  it('combines armor, profile and noOsSandbox flags correctly', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], {
      armor: './armor.json',
      profile: 'strict',
      noOsSandbox: true,
    });
    const args = capturedSpawnCalls[0]?.args ?? [];
    assert.ok(args.includes('--armor'));
    assert.ok(args.includes('./armor.json'));
    assert.ok(args.includes('--profile'));
    assert.ok(args.includes('strict'));
    assert.ok(args.includes('--no-os-sandbox'));
  });
});

// ---------------------------------------------------------------------------
// SpawnOptions forwarding
// ---------------------------------------------------------------------------

describe('armorSpawn — SpawnOptions forwarding', () => {
  it('forwards cwd to the underlying spawn call', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { cwd: '/some/working/dir' });
    assert.equal(capturedSpawnCalls[0]?.options['cwd'], '/some/working/dir');
  });

  it('forwards env to the underlying spawn call', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    const env = { PATH: '/usr/bin', MY_VAR: 'hello' };
    armorSpawn(['node', 'tool.js'], { env });
    assert.deepEqual(capturedSpawnCalls[0]?.options['env'], env);
  });

  it('forwards stdio to the underlying spawn call', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { stdio: 'inherit' });
    assert.equal(capturedSpawnCalls[0]?.options['stdio'], 'inherit');
  });

  it('does NOT forward armor to the underlying spawn options', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { armor: './armor.json' });
    assert.ok(!('armor' in (capturedSpawnCalls[0]?.options ?? {})));
  });

  it('does NOT forward profile to the underlying spawn options', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { profile: 'strict' });
    assert.ok(!('profile' in (capturedSpawnCalls[0]?.options ?? {})));
  });

  it('does NOT forward noOsSandbox to the underlying spawn options', async () => {
    const { armorSpawn } = await loadSpawnWithMocks({ binaryResult: FAKE_BINARY });
    armorSpawn(['node', 'tool.js'], { noOsSandbox: true });
    assert.ok(!('noOsSandbox' in (capturedSpawnCalls[0]?.options ?? {})));
  });
});

// ---------------------------------------------------------------------------
// BinaryNotFoundError propagation
// ---------------------------------------------------------------------------

describe('armorSpawn — BinaryNotFoundError propagation', () => {
  it('propagates BinaryNotFoundError thrown by findBinary', async () => {
    // Reset any prior mocks so that the binary.ts import resolves without a
    // mocked node:child_process that omits execFileSync.
    mock.reset();
    const { BinaryNotFoundError } = await import('../src/binary.ts?nocache=' + Math.random());
    const err = new BinaryNotFoundError();
    const { armorSpawn } = await loadSpawnWithMocks({ binaryThrows: err });

    assert.throws(
      () => armorSpawn(['node', 'tool.js']),
      (thrown: unknown) => thrown instanceof Error && thrown.name === 'BinaryNotFoundError',
    );
  });
});
