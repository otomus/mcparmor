#!/usr/bin/env python3
"""Adversarial MCP tool: call_forbidden (Python variant).

Exposes an http_fetch tool. The test runner calls it with a URL to
evil.example.com, which is not in the network.allow list. The broker must
block this at Layer 1 before the tool sees the request.

This Python variant demonstrates that the broker's network param inspection
works regardless of the tool's implementation language.
"""
from __future__ import annotations

import json
import sys
from typing import Any


def send(message: dict[str, Any]) -> None:
    """Write a JSON-RPC message to stdout."""
    print(json.dumps(message), flush=True)


def handle_initialize(req: dict[str, Any]) -> None:
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "serverInfo": {"name": "call-forbidden-tool-py", "version": "1.0"},
        },
    })


def handle_tools_list(req: dict[str, Any]) -> None:
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "tools": [
                {
                    "name": "http_fetch",
                    "description": "Fetch a URL over HTTP",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "url": {"type": "string"},
                        },
                        "required": ["url"],
                    },
                }
            ]
        },
    })


def handle_tools_call(req: dict[str, Any]) -> None:
    # The broker must block evil.example.com before reaching us.
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": "REACHED_TOOL: call was not blocked by broker",
                }
            ]
        },
    })


def main() -> None:
    """Read JSON-RPC messages from stdin and dispatch them."""
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        method = req.get("method", "")

        if method == "initialize":
            handle_initialize(req)
        elif method == "notifications/initialized":
            # Notification — no response.
            pass
        elif method == "tools/list":
            handle_tools_list(req)
        elif method == "tools/call":
            handle_tools_call(req)


if __name__ == "__main__":
    main()
