#!/usr/bin/env node
/**
 * Adversarial MCP tool: spawn_child (Node.js variant).
 *
 * Exposes a run_command tool that attempts to spawn a child process (ls) and
 * return its output. This tests OS-level spawn blocking:
 *
 *   - macOS (Seatbelt, spawn: false): (deny process-exec) in the SBPL profile
 *     prevents exec() from succeeding — the tool returns SPAWN_BLOCKED.
 *   - Linux (Seccomp, spawn: false): Seccomp blocks execve/execveat syscalls on
 *     kernels where the broker successfully installs the filter. The outcome is
 *     BLOCKED when Seccomp is active.
 *
 * In both cases the broker does not intercept this at Layer 1 (no path or URL
 * argument is sent), so this test exercises Layer 2 only.
 *
 * This Node.js variant uses child_process.spawnSync() which ultimately calls
 * execve() at the kernel level — the same syscall path that Seccomp and
 * Seatbelt block.
 */

'use strict';

const readline = require('readline');
const { spawnSync } = require('child_process');

/**
 * Write a JSON-RPC message to stdout.
 *
 * @param {object} message - The JSON-RPC message to send.
 */
function send(message) {
  process.stdout.write(JSON.stringify(message) + '\n');
}

/**
 * Attempt to spawn a child process.
 *
 * Returns a string prefixed with SPAWN_BLOCKED if the OS sandbox blocked
 * the exec, or SPAWN_SUCCESS with the command output if it succeeded.
 *
 * @returns {string} Result string with SPAWN_BLOCKED or SPAWN_SUCCESS prefix.
 */
function trySpawnChild() {
  const result = spawnSync('ls', ['/tmp'], { encoding: 'utf8', timeout: 5000 });

  if (result.error) {
    return 'SPAWN_BLOCKED: ' + result.error.message;
  }
  if (result.status !== 0) {
    const stderr = result.stderr ? result.stderr.trim() : '';
    return 'SPAWN_BLOCKED: exit ' + result.status + (stderr ? ': ' + stderr : '');
  }
  return 'SPAWN_SUCCESS: ' + (result.stdout ? result.stdout.trim() : '');
}

/**
 * Handle an MCP initialize request.
 *
 * @param {object} req - The incoming JSON-RPC request.
 */
function handleInitialize(req) {
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: {
      protocolVersion: '2024-11-05',
      capabilities: {},
      serverInfo: { name: 'spawn-child-tool-js', version: '1.0' },
    },
  });
}

/**
 * Handle a tools/list request.
 *
 * @param {object} req - The incoming JSON-RPC request.
 */
function handleToolsList(req) {
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: {
      tools: [
        {
          name: 'run_command',
          description: 'Run a shell command and return its output',
          inputSchema: {
            type: 'object',
            properties: {},
          },
        },
      ],
    },
  });
}

/**
 * Handle a tools/call request.
 *
 * Attempts to spawn a child process and returns the result.
 *
 * @param {object} req - The incoming JSON-RPC request.
 */
function handleToolsCall(req) {
  const result = trySpawnChild();
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: {
      content: [
        {
          type: 'text',
          text: result,
        },
      ],
    },
  });
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  let req;
  try {
    req = JSON.parse(trimmed);
  } catch (_) {
    return;
  }

  switch (req.method) {
    case 'initialize':
      handleInitialize(req);
      break;
    case 'notifications/initialized':
      // Notification — no response.
      break;
    case 'tools/list':
      handleToolsList(req);
      break;
    case 'tools/call':
      handleToolsCall(req);
      break;
    default:
      break;
  }
});
