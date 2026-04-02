#!/usr/bin/env python3
"""Mock MCP tool server for the mcparmor testkit.

A lightweight MCP-compliant server that reads JSON-RPC messages from stdin
and responds with user-configured responses. Used as the "tool behind the
broker" in test harnesses — the broker enforces policy against this server's
responses.

Configuration is loaded from a JSON file whose path is passed as the
first CLI argument. The config is re-read on every request so that
mid-test reconfiguration works without restarting the process.
"""
from __future__ import annotations

import json
import sys
from typing import Any

def _resolve_config_path() -> str:
    """Determine the config file path from CLI args.

    The path is passed as the first positional argument by the test harness.

    Returns:
        Config file path, or empty string if not provided.
    """
    if len(sys.argv) > 1:
        return sys.argv[1]
    return ""


def _load_config(config_path: str) -> dict[str, Any]:
    """Load the mock configuration from disk.

    Re-reads on every call so the harness can update responses mid-test.

    Args:
        config_path: Absolute path to the JSON config file.

    Returns:
        Parsed configuration dictionary.
    """
    if not config_path:
        return {}
    with open(config_path) as fh:
        return json.load(fh)


def _send(message: dict[str, Any]) -> None:
    """Write a JSON-RPC message to stdout."""
    print(json.dumps(message), flush=True)


def _handle_initialize(req: dict[str, Any], config: dict[str, Any]) -> None:
    server_info = config.get("server_info", {
        "name": "mcparmor-mock-tool",
        "version": "1.0",
    })
    _send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "serverInfo": server_info,
        },
    })


def _handle_tools_list(req: dict[str, Any], config: dict[str, Any]) -> None:
    tools = config.get("tools", [])
    _send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {"tools": tools},
    })


def _handle_tools_call(req: dict[str, Any], config: dict[str, Any]) -> None:
    tool_name = req.get("params", {}).get("name", "")
    responses = config.get("responses", {})
    default_response = config.get("default_response", {
        "content": [{"type": "text", "text": "mock response"}],
    })
    result = responses.get(tool_name, default_response)
    _send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": result,
    })


def main() -> None:
    """Read JSON-RPC messages from stdin and dispatch them."""
    config_path = _resolve_config_path()

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        method = req.get("method", "")
        # Re-read config on every request so mid-test changes take effect.
        config = _load_config(config_path)

        if method == "initialize":
            _handle_initialize(req, config)
        elif method == "notifications/initialized":
            pass
        elif method == "tools/list":
            _handle_tools_list(req, config)
        elif method == "tools/call":
            _handle_tools_call(req, config)


if __name__ == "__main__":
    main()
