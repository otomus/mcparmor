#!/usr/bin/env node
/**
 * Mock MCP tool server for the mcparmor testkit.
 *
 * A lightweight MCP-compliant server that reads JSON-RPC messages from stdin
 * and responds with user-configured responses. Used as the "tool behind the
 * broker" in test harnesses.
 *
 * Configuration is loaded from a JSON file whose path is passed as the
 * first CLI argument. The config is re-read on every request so that
 * mid-test reconfiguration works without restarting.
 */

import { createInterface } from 'node:readline';
import { readFileSync } from 'node:fs';

/** Resolve config path from CLI argument (passed by the test harness). */
const CONFIG_PATH = process.argv[2] || '';

/**
 * Load the mock configuration from disk.
 * Re-reads on every call so the harness can update responses mid-test.
 *
 * @returns {object} Parsed configuration object.
 */
function loadConfig() {
  if (!CONFIG_PATH) {
    return {};
  }
  return JSON.parse(readFileSync(CONFIG_PATH, 'utf8'));
}

/**
 * Write a JSON-RPC message to stdout.
 *
 * @param {object} message - The JSON-RPC response to send.
 */
function send(message) {
  process.stdout.write(JSON.stringify(message) + '\n');
}

/**
 * Handle an initialize request.
 *
 * @param {object} req - The JSON-RPC request.
 * @param {object} config - The mock configuration.
 */
function handleInitialize(req, config) {
  const serverInfo = config.server_info || {
    name: 'mcparmor-mock-tool',
    version: '1.0',
  };
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: {
      protocolVersion: '2024-11-05',
      capabilities: {},
      serverInfo,
    },
  });
}

/**
 * Handle a tools/list request.
 *
 * @param {object} req - The JSON-RPC request.
 * @param {object} config - The mock configuration.
 */
function handleToolsList(req, config) {
  const tools = config.tools || [];
  send({
    jsonrpc: '2.0',
    id: req.id,
    result: { tools },
  });
}

/**
 * Handle a tools/call request.
 *
 * @param {object} req - The JSON-RPC request.
 * @param {object} config - The mock configuration.
 */
function handleToolsCall(req, config) {
  const toolName = req.params?.name || '';
  const responses = config.responses || {};
  const defaultResponse = config.default_response || {
    content: [{ type: 'text', text: 'mock response' }],
  };
  const result = responses[toolName] || defaultResponse;
  send({
    jsonrpc: '2.0',
    id: req.id,
    result,
  });
}

const rl = createInterface({ input: process.stdin });

rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) {
    return;
  }

  let req;
  try {
    req = JSON.parse(trimmed);
  } catch {
    return;
  }

  // Re-read config on every request so mid-test changes take effect.
  const config = loadConfig();
  const method = req.method || '';

  if (method === 'initialize') {
    handleInitialize(req, config);
  } else if (method === 'notifications/initialized') {
    // Notification — no response.
  } else if (method === 'tools/list') {
    handleToolsList(req, config);
  } else if (method === 'tools/call') {
    handleToolsCall(req, config);
  }
});
