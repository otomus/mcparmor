'use strict';

/**
 * weather — OpenClaw skill that fetches current weather for a city.
 *
 * MCP tool interface: reads JSON-RPC requests from stdin, writes responses
 * to stdout. The armor manifest (armor.json) in this directory declares that
 * this skill may only reach api.openweathermap.org:443 and api.open-meteo.com:443.
 * All other network access, filesystem writes, and subprocess spawning are denied.
 *
 * Supported methods:
 *   - tools/list      — returns the list of available tools
 *   - tools/call      — dispatches to the appropriate tool handler
 *
 * Tools:
 *   - get_weather     — returns current weather for a given city name
 */

const https = require('https');
const readline = require('readline');

const OPENWEATHER_API_KEY = process.env.OPENWEATHER_API_KEY ?? '';
const OPENWEATHER_BASE_URL = 'https://api.openweathermap.org/data/2.5/weather';

/**
 * Fetches current weather data for a city from the OpenWeatherMap API.
 *
 * @param {string} city - City name to look up
 * @returns {Promise<{ temperature: number, description: string, humidity: number }>}
 * @throws {Error} If the API request fails or the city is not found
 */
function fetchWeather(city) {
  return new Promise((resolve, reject) => {
    const url = `${OPENWEATHER_BASE_URL}?q=${encodeURIComponent(city)}&appid=${OPENWEATHER_API_KEY}&units=metric`;
    https.get(url, (res) => {
      let body = '';
      res.on('data', (chunk) => { body += chunk; });
      res.on('end', () => {
        if (res.statusCode !== 200) {
          reject(new Error(`Weather API error: ${res.statusCode} ${body}`));
          return;
        }
        const data = JSON.parse(body);
        resolve({
          temperature: data.main.temp,
          description: data.weather[0].description,
          humidity: data.main.humidity,
        });
      });
    }).on('error', reject);
  });
}

/**
 * Handles a tools/list request.
 *
 * @returns {object} MCP tools/list result
 */
function handleToolsList() {
  return {
    tools: [
      {
        name: 'get_weather',
        description: 'Get the current weather for a city.',
        inputSchema: {
          type: 'object',
          properties: {
            city: { type: 'string', description: 'City name (e.g. "London")' },
          },
          required: ['city'],
        },
      },
    ],
  };
}

/**
 * Handles a tools/call request by dispatching to the named tool.
 *
 * @param {string} toolName - Name of the tool to invoke
 * @param {Record<string, unknown>} toolArgs - Arguments for the tool
 * @returns {Promise<object>} MCP tools/call result
 */
async function handleToolCall(toolName, toolArgs) {
  if (toolName !== 'get_weather') {
    throw new Error(`Unknown tool: ${toolName}`);
  }
  if (typeof toolArgs.city !== 'string' || toolArgs.city.trim() === '') {
    throw new Error('get_weather requires a non-empty city argument');
  }
  const weather = await fetchWeather(toolArgs.city.trim());
  return {
    content: [
      {
        type: 'text',
        text: `Weather in ${toolArgs.city}: ${weather.temperature}°C, ${weather.description}, humidity ${weather.humidity}%.`,
      },
    ],
  };
}

/**
 * Builds a JSON-RPC 2.0 success response.
 *
 * @param {number | string} id
 * @param {unknown} result
 * @returns {string} Serialized JSON-RPC response
 */
function successResponse(id, result) {
  return JSON.stringify({ jsonrpc: '2.0', id, result });
}

/**
 * Builds a JSON-RPC 2.0 error response.
 *
 * @param {number | string | null} id
 * @param {number} code
 * @param {string} message
 * @returns {string} Serialized JSON-RPC response
 */
function errorResponse(id, code, message) {
  return JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } });
}

/**
 * Processes a single parsed JSON-RPC request and returns the response string.
 *
 * @param {object} request - Parsed JSON-RPC request object
 * @returns {Promise<string>} Serialized JSON-RPC response
 */
async function processRequest(request) {
  const { id, method, params } = request;
  try {
    if (method === 'tools/list') {
      return successResponse(id, handleToolsList());
    }
    if (method === 'tools/call') {
      const result = await handleToolCall(params.name, params.arguments ?? {});
      return successResponse(id, result);
    }
    return errorResponse(id, -32601, `Method not found: ${method}`);
  } catch (err) {
    return errorResponse(id, -32000, err.message);
  }
}

/** Reads newline-delimited JSON-RPC requests from stdin and writes responses to stdout. */
async function main() {
  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of rl) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    let request;
    try {
      request = JSON.parse(trimmed);
    } catch {
      process.stdout.write(errorResponse(null, -32700, 'Parse error') + '\n');
      continue;
    }
    const response = await processRequest(request);
    process.stdout.write(response + '\n');
  }
}

main().catch((err) => {
  process.stderr.write(`Fatal error: ${err.message}\n`);
  process.exit(1);
});
