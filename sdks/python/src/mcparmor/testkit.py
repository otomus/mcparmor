"""MCP Armor TestKit — test your armor.json policies against the real broker.

Provides :class:`ArmorTestHarness`, a context manager that spins up the real
mcparmor broker backed by a lightweight mock MCP tool server. You define what
responses the mock tool returns; the harness sends ``tools/call`` messages
through the broker and reports whether they were blocked, allowed, or had
secrets redacted.

Example::

    async with ArmorTestHarness(armor="./armor.json") as harness:
        harness.mock_tool_response({
            "content": [{"type": "text", "text": "hello"}]
        })
        result = await harness.call_tool("read_file", {"path": "/etc/passwd"})
        assert result.blocked
        assert result.error_code == ArmorErrorCode.PATH_VIOLATION
"""
from __future__ import annotations

import asyncio
import json
import logging
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from types import TracebackType
from typing import Any

from mcparmor._binary import find_binary

_logger = logging.getLogger(__name__)


class ArmorErrorCode:
    """JSON-RPC error codes returned by the mcparmor broker on policy violations."""

    PATH_VIOLATION = -32001
    NETWORK_VIOLATION = -32002
    SPAWN_VIOLATION = -32003
    SECRET_BLOCKED = -32004
    TIMEOUT = -32005


@dataclass
class ToolCallResult:
    """Outcome of sending a ``tools/call`` message through the broker.

    Attributes:
        raw: The full JSON-RPC response envelope.
        blocked: True if the broker returned an error (policy violation).
        allowed: True if the call passed through to the mock tool.
        error_code: The JSON-RPC error code, or None if not blocked.
        error_message: The error message string, or None if not blocked.
        response: The ``result`` payload from the mock tool, or None if blocked.
    """

    raw: dict[str, Any]
    blocked: bool
    allowed: bool
    error_code: int | None = None
    error_message: str | None = None
    response: dict[str, Any] | None = None

    @property
    def text(self) -> str | None:
        """Extract the first text content from the tool response.

        Returns:
            The text string if the response has MCP content items, None otherwise.
        """
        if self.response is None:
            return None
        content = self.response.get("content", [])
        if not content:
            return None
        return content[0].get("text")


class ArmorTestHarnessError(Exception):
    """Raised when the test harness cannot be started or communication fails."""


class ArmorTestHarness:
    """Test harness that runs the real mcparmor broker with a mock tool behind it.

    Use as an async context manager to manage the broker lifecycle::

        async with ArmorTestHarness(armor="./armor.json") as harness:
            result = await harness.call_tool("my_tool", {"key": "value"})

    Args:
        armor: Path to the armor.json manifest to test.
        profile: Optional profile override (e.g. ``"strict"``).
        no_os_sandbox: Disable Layer 2 OS sandbox. Defaults to True because
            testkit tests Layer 1 enforcement; the OS sandbox would interfere
            with the mock server.
        timeout: Read timeout in seconds for individual tool calls.
    """

    def __init__(
        self,
        *,
        armor: str | Path,
        profile: str | None = None,
        no_os_sandbox: bool = True,
        timeout: float = 10.0,
    ) -> None:
        self._armor = str(armor)
        self._profile = profile
        self._no_os_sandbox = no_os_sandbox
        self._timeout = timeout
        self._process: subprocess.Popen[bytes] | None = None
        self._tmp_dir: tempfile.TemporaryDirectory[str] | None = None
        self._config_path: str = ""
        self._next_id_counter = 0
        self._config: dict[str, Any] = {
            "server_info": {"name": "mcparmor-mock-tool", "version": "1.0"},
            "tools": [],
            "default_response": {
                "content": [{"type": "text", "text": "mock response"}],
            },
            "responses": {},
        }

    # ------------------------------------------------------------------
    # Context manager
    # ------------------------------------------------------------------

    async def __aenter__(self) -> ArmorTestHarness:
        """Start the broker process and perform the MCP handshake."""
        self._start()
        await self._handshake()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        """Terminate the broker process and clean up temp files."""
        self.stop()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def mock_tool_response(
        self,
        response: dict[str, Any],
        *,
        tool_name: str | None = None,
    ) -> None:
        """Set the response the mock tool returns for ``tools/call``.

        Args:
            response: The MCP result payload (e.g.
                ``{"content": [{"type": "text", "text": "..."}]}``).
            tool_name: If given, this response is used only when the tool call
                targets this specific tool name. Otherwise, it becomes the
                default response for all tools.
        """
        if tool_name is not None:
            self._config["responses"][tool_name] = response
        else:
            self._config["default_response"] = response
        self._write_config()

    def set_tools(self, tools: list[dict[str, Any]]) -> None:
        """Set the tool definitions returned by ``tools/list``.

        Args:
            tools: List of MCP tool definition objects (name, description,
                inputSchema).
        """
        self._config["tools"] = tools
        self._write_config()

    async def call_tool(
        self,
        name: str,
        arguments: dict[str, Any] | None = None,
    ) -> ToolCallResult:
        """Send a ``tools/call`` JSON-RPC message through the broker.

        Args:
            name: The tool name to call.
            arguments: The arguments to pass to the tool.

        Returns:
            A :class:`ToolCallResult` describing whether the call was blocked
            or allowed, and the response payload.

        Raises:
            ArmorTestHarnessError: If the broker process is not running.
        """
        if arguments is None:
            arguments = {}

        message = {
            "jsonrpc": "2.0",
            "id": self._next_id(),
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
            },
        }
        raw_response = await self._send_and_receive(message)
        return _classify_response(raw_response)

    async def send_raw(self, message: dict[str, Any]) -> dict[str, Any]:
        """Send an arbitrary JSON-RPC message and return the raw response.

        Useful for testing non-``tools/call`` interactions (e.g. ``tools/list``).

        Args:
            message: A JSON-RPC request object. Must include ``id`` if a
                response is expected.

        Returns:
            The parsed JSON-RPC response dictionary.
        """
        return await self._send_and_receive(message)

    def stop(self) -> None:
        """Terminate the broker and clean up resources.

        Safe to call multiple times.
        """
        if self._process is not None:
            try:
                self._process.terminate()
                self._process.wait(timeout=5)
            except OSError:
                _logger.warning("Failed to terminate broker cleanly")
            finally:
                self._process = None
        if self._tmp_dir is not None:
            self._tmp_dir.cleanup()
            self._tmp_dir = None

    # ------------------------------------------------------------------
    # Private helpers
    # ------------------------------------------------------------------

    def _next_id(self) -> int:
        """Return an auto-incrementing JSON-RPC message ID."""
        self._next_id_counter += 1
        return self._next_id_counter

    def _write_config(self) -> None:
        """Persist the current mock config to disk atomically.

        Writes to a temp file first then renames, so the mock server never
        sees a truncated/empty config file.
        """
        tmp_path = self._config_path + ".tmp"
        with open(tmp_path, "w") as fh:
            json.dump(self._config, fh)
        os.replace(tmp_path, self._config_path)

    def _start(self) -> None:
        """Start the broker with the mock tool server behind it."""
        self._tmp_dir = tempfile.TemporaryDirectory(prefix="mcparmor-testkit-")
        self._config_path = os.path.join(self._tmp_dir.name, "mock_config.json")
        self._write_config()

        binary = str(find_binary())
        mock_server = str(
            Path(__file__).parent / "_mock_tool_server.py"
        )

        broker_args = ["run"]
        broker_args.extend(["--armor", self._armor])
        if self._profile is not None:
            broker_args.extend(["--profile", self._profile])
        if self._no_os_sandbox:
            broker_args.append("--no-os-sandbox")
        broker_args.extend(["--no-audit", "--"])
        broker_args.extend([sys.executable, mock_server, self._config_path])

        env = os.environ.copy()

        try:
            self._process = subprocess.Popen(
                [binary, *broker_args],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                env=env,
            )
        except OSError as exc:
            raise ArmorTestHarnessError(
                f"Failed to start mcparmor broker: {exc}"
            ) from exc

    async def _handshake(self) -> None:
        """Perform the MCP initialize / notifications/initialized handshake."""
        init_msg = {
            "jsonrpc": "2.0",
            "id": self._next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "mcparmor-testkit", "version": "1.0"},
            },
        }
        response = await self._send_and_receive(init_msg)
        if "error" in response:
            raise ArmorTestHarnessError(
                f"MCP handshake failed: {response['error']}"
            )

        proc = self._process
        if proc is None or proc.stdin is None:
            raise ArmorTestHarnessError("Broker process is not running.")
        notification = json.dumps({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }) + "\n"
        proc.stdin.write(notification.encode())
        proc.stdin.flush()

    async def _send_and_receive(
        self, message: dict[str, Any]
    ) -> dict[str, Any]:
        """Write a message and read one JSON-RPC response line.

        Performs both the write and blocking read in a thread to avoid
        blocking the event loop and to ensure atomicity.

        Args:
            message: JSON-RPC request to send.

        Returns:
            Parsed JSON-RPC response.

        Raises:
            ArmorTestHarnessError: If the broker is not running or the read
                times out.
        """
        loop = asyncio.get_running_loop()
        try:
            return await asyncio.wait_for(
                loop.run_in_executor(
                    None, self._sync_send_and_receive, message
                ),
                timeout=self._timeout,
            )
        except asyncio.TimeoutError:
            raise ArmorTestHarnessError(
                "Timed out waiting for broker response."
            )

    def _sync_send_and_receive(
        self, message: dict[str, Any]
    ) -> dict[str, Any]:
        """Write a message and read one response, synchronously.

        Args:
            message: JSON-RPC request to send.

        Returns:
            Parsed JSON-RPC response dict.
        """
        proc = self._process
        if proc is None or proc.stdin is None or proc.stdout is None:
            raise ArmorTestHarnessError("Broker process is not running.")

        line = json.dumps(message) + "\n"
        proc.stdin.write(line.encode())
        proc.stdin.flush()

        raw = proc.stdout.readline()
        if not raw:
            raise ArmorTestHarnessError(
                "Broker closed stdout without responding."
            )

        text = raw.decode().strip()
        try:
            return json.loads(text)
        except json.JSONDecodeError as exc:
            raise ArmorTestHarnessError(
                f"Broker returned invalid JSON: {text}"
            ) from exc


def _classify_response(raw: dict[str, Any]) -> ToolCallResult:
    """Classify a JSON-RPC response as blocked or allowed.

    Args:
        raw: The full JSON-RPC response envelope.

    Returns:
        A populated :class:`ToolCallResult`.
    """
    if "error" in raw:
        error = raw["error"]
        return ToolCallResult(
            raw=raw,
            blocked=True,
            allowed=False,
            error_code=error.get("code"),
            error_message=error.get("message"),
        )
    return ToolCallResult(
        raw=raw,
        blocked=False,
        allowed=True,
        response=raw.get("result"),
    )
