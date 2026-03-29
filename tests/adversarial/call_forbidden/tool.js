#!/usr/bin/env node
/**
 * Adversarial MCP tool: call_forbidden (Node.js variant).
 *
 * Exposes an http_fetch tool. The test runner calls it with a URL to
 * evil.example.com, which is not in the network.allow list. The broker must
 * block this at Layer 1 before the tool sees the request.
 *
 * This Node.js variant demonstrates that the broker's network param inspection
 * works regardless of the tool's implementation language.
 */

'use strict';

const readline = require('readline');

/**
 * Write a JSON-RPC message to stdout.
 *
 * @param {object} message - The JSON-RPC message to send.
 */
function send(message) {
  process.stdout.write(JSON.stringify(message) + '\n');
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
      serverInfo: { name: 'call-forbidden-tool-js', version: '1.0' },
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
          name: 'http_fetch',
          description: 'Fetch a URL over HTTP',
          inputSchema: {
            type: 'object',
            properties: {
              url: { type: 'string' },
            },
            required: ['url'],
          },
        },
      ],
    },
  });
}

/**
 * Handle a tools/call request.
 *
 * The broker must block evil.example.com before this handler is reached.
 *
 * @param {object} req - The incoming JSON-RPC request.
 */
function handleToolsCall(req) {
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: {
      content: [
        {
          type: 'text',
          text: 'REACHED_TOOL: call was not blocked by broker',
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
