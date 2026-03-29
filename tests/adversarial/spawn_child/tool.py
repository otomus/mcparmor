#!/usr/bin/env python3
"""Adversarial MCP tool: spawn_child (Python variant).

Exposes a run_command tool that attempts to spawn a child process (ls) and
return its output. This tests OS-level spawn blocking:

  - macOS (Seatbelt, spawn: false): (deny process-exec) in the SBPL profile
    prevents exec() from succeeding — the tool returns SPAWN_BLOCKED.
  - Linux (Seccomp, spawn: false): Seccomp blocks execve/execveat syscalls on
    kernels where the broker successfully installs the filter. The outcome is
    BLOCKED when Seccomp is active.

In both cases the broker does not intercept this at Layer 1 (no path or URL
argument is sent), so this test exercises Layer 2 only.

This Python variant uses subprocess.run() which ultimately calls os.execve()
at the C/kernel level — the same syscall path that Seccomp and Seatbelt block.
"""
from __future__ import annotations

import json
import subprocess
import sys
from typing import Any


def send(message: dict[str, Any]) -> None:
    """Write a JSON-RPC message to stdout."""
    print(json.dumps(message), flush=True)


def try_spawn_child() -> str:
    """Attempt to spawn a child process.

    Returns a string prefixed with SPAWN_BLOCKED if the OS sandbox blocked
    the exec, or SPAWN_SUCCESS with the command output if it succeeded.
    """
    try:
        result = subprocess.run(
            ["ls", "/tmp"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        return "SPAWN_SUCCESS: " + result.stdout.strip()
    except (OSError, PermissionError) as exc:
        return "SPAWN_BLOCKED: " + str(exc)
    except subprocess.TimeoutExpired:
        return "SPAWN_BLOCKED: timeout"


def handle_initialize(req: dict[str, Any]) -> None:
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "serverInfo": {"name": "spawn-child-tool-py", "version": "1.0"},
        },
    })


def handle_tools_list(req: dict[str, Any]) -> None:
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "tools": [
                {
                    "name": "run_command",
                    "description": "Run a shell command and return its output",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                    },
                }
            ]
        },
    })


def handle_tools_call(req: dict[str, Any]) -> None:
    result = try_spawn_child()
    send({
        "jsonrpc": "2.0",
        "id": req.get("id"),
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": result,
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
