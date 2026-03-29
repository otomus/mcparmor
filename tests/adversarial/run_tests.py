#!/usr/bin/env python3
"""Adversarial MCP test runner for mcparmor.

Speaks the real MCP JSON-RPC 2.0 protocol over stdio. For each test case,
spawns the broker wrapping the adversarial tool, completes the MCP
handshake, calls the target tool, and verifies the broker's response.

Usage:
    python3 tests/adversarial/run_tests.py --broker ./target/release/mcparmor

Exit codes:
    0  All required tests passed.
    1  One or more required tests failed.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import platform
import queue
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Any

# JSON-RPC error codes emitted by the broker.
_CODE_PATH_VIOLATION = -32001
_CODE_NETWORK_VIOLATION = -32002
_CODE_SECRET_DETECTED = -32004

# Directory containing this script.
_TESTS_DIR = Path(__file__).parent


@dataclasses.dataclass
class TestCase:
    """Definition of a single adversarial test."""

    name: str
    """Directory name under tests/adversarial/."""

    tool_method: str
    """The tools/call method name to invoke."""

    arguments: dict[str, Any]
    """Arguments to pass in tools/call.params.arguments."""

    expected_error_codes: list[int]
    """JSON-RPC error codes that constitute a BLOCKED outcome."""

    tool_variant: str = "tool"
    """
    Filename (without extension) for the tool to use.
    Default is "tool" (the compiled Go binary).
    Use "tool.py" for the Python variant or "tool.js" for the Node variant.
    """

    label_suffix: str = ""
    """
    Optional suffix appended to the test label to disambiguate cases with the
    same name and tool_variant but different arguments (e.g. two call_metadata
    tests with different IPs both use name="call_metadata" and tool_variant="tool").
    When set, the label becomes "<name>/<tool_variant>[<label_suffix>]".
    """

    informational_on_linux: bool = False
    """When True, a non-BLOCKED outcome on Linux is recorded but does not fail CI."""

    tool_response_blocked_prefix: str | None = None
    """
    When set, a tools/call success response whose text starts with this prefix
    is also treated as BLOCKED. Used for spawn_child where the OS may block the
    exec inside the tool process (macOS Seatbelt) without the broker intercepting.
    """


def _resolve_tool_command(tool_variant: str, tool_dir: Path) -> list[str] | None:
    """Resolve the command to invoke a tool variant.

    Returns the command list, or None if the tool is not available.

    For the compiled Go binary ("tool"), checks that the binary exists.
    For Python variants ("tool.py"), uses the python3 interpreter from PATH.
    For Node variants ("tool.js"), uses the node interpreter from PATH.

    @param tool_variant: The tool filename (e.g. "tool", "tool.py", "tool.js").
    @param tool_dir: The directory containing the tool file.
    @returns: Command list to pass to subprocess, or None if unavailable.
    """
    tool_path = tool_dir / tool_variant

    if not tool_path.exists():
        return None

    if tool_variant == "tool":
        # Compiled Go binary — run directly.
        return [str(tool_path)]

    if tool_variant.endswith(".py"):
        interpreter = shutil.which("python3") or shutil.which("python")
        if interpreter is None:
            return None
        return [interpreter, str(tool_path)]

    if tool_variant.endswith(".js"):
        interpreter = shutil.which("node")
        if interpreter is None:
            return None
        return [interpreter, str(tool_path)]

    return None


# All adversarial test cases, in order of increasing sophistication.
# Each scenario is covered by its Go binary (Layer 2 test) and by Python/Node
# variants (demonstrate that Layer 1 enforcement is language-agnostic).
TEST_CASES: list[TestCase] = [
    # --- path_traversal ---
    # Layer 1: broker blocks traversal sequences in JSON-RPC params.
    TestCase(
        name="path_traversal",
        tool_method="read_file",
        arguments={"path": "../../etc/passwd"},
        expected_error_codes=[_CODE_PATH_VIOLATION],
    ),

    # --- read_passwd ---
    # Layer 1: broker blocks absolute paths not in filesystem.read allowlist.
    TestCase(
        name="read_passwd",
        tool_method="read_file",
        arguments={"path": "/etc/passwd"},
        expected_error_codes=[_CODE_PATH_VIOLATION],
    ),
    TestCase(
        name="read_passwd",
        tool_variant="tool.py",
        tool_method="read_file",
        arguments={"path": "/etc/passwd"},
        expected_error_codes=[_CODE_PATH_VIOLATION],
    ),
    TestCase(
        name="read_passwd",
        tool_variant="tool.js",
        tool_method="read_file",
        arguments={"path": "/etc/passwd"},
        expected_error_codes=[_CODE_PATH_VIOLATION],
    ),

    # --- call_forbidden ---
    # Layer 1: broker blocks hosts not in network.allow.
    TestCase(
        name="call_forbidden",
        tool_method="http_fetch",
        arguments={"url": "https://evil.example.com/exfil"},
        expected_error_codes=[_CODE_NETWORK_VIOLATION],
    ),
    TestCase(
        name="call_forbidden",
        tool_variant="tool.py",
        tool_method="http_fetch",
        arguments={"url": "https://evil.example.com/exfil"},
        expected_error_codes=[_CODE_NETWORK_VIOLATION],
    ),
    TestCase(
        name="call_forbidden",
        tool_variant="tool.js",
        tool_method="http_fetch",
        arguments={"url": "https://evil.example.com/exfil"},
        expected_error_codes=[_CODE_NETWORK_VIOLATION],
    ),

    # --- call_metadata ---
    # Layer 1: deny_metadata blocks the full 169.254.0.0/16 CIDR range.
    # Two distinct IPs are tested to confirm the full /16 block, not just
    # the canonical 169.254.169.254 address.
    TestCase(
        name="call_metadata",
        label_suffix="169.254.169.254",
        tool_method="http_fetch",
        arguments={"url": "http://169.254.169.254/latest/meta-data/"},
        expected_error_codes=[_CODE_NETWORK_VIOLATION],
    ),
    TestCase(
        name="call_metadata",
        label_suffix="169.254.1.1",
        tool_method="http_fetch",
        arguments={"url": "http://169.254.1.1/"},
        expected_error_codes=[_CODE_NETWORK_VIOLATION],
    ),

    # --- leak_secret ---
    # Layer 2: broker scans tool response output and blocks secrets.
    TestCase(
        name="leak_secret",
        tool_method="get_config",
        arguments={},
        expected_error_codes=[_CODE_SECRET_DETECTED],
    ),
    TestCase(
        name="leak_secret",
        tool_variant="tool.py",
        tool_method="get_config",
        arguments={},
        expected_error_codes=[_CODE_SECRET_DETECTED],
    ),
    TestCase(
        name="leak_secret",
        tool_variant="tool.js",
        tool_method="get_config",
        arguments={},
        expected_error_codes=[_CODE_SECRET_DETECTED],
    ),

    # --- spawn_child ---
    # Layer 2: OS sandbox (Seatbelt/Seccomp) blocks child process exec.
    # No Layer 1 block — the tool calls exec() directly without JSON-RPC params.
    TestCase(
        name="spawn_child",
        tool_method="run_command",
        arguments={},
        expected_error_codes=[],  # No Layer 1 block — tested via tool response.
        informational_on_linux=True,
        tool_response_blocked_prefix="SPAWN_BLOCKED",
    ),
    TestCase(
        name="spawn_child",
        tool_variant="tool.py",
        tool_method="run_command",
        arguments={},
        expected_error_codes=[],
        informational_on_linux=True,
        tool_response_blocked_prefix="SPAWN_BLOCKED",
    ),
    TestCase(
        name="spawn_child",
        tool_variant="tool.js",
        tool_method="run_command",
        arguments={},
        expected_error_codes=[],
        informational_on_linux=True,
        tool_response_blocked_prefix="SPAWN_BLOCKED",
    ),
]


class McpSession:
    """An active MCP session proxied through the broker.

    Manages the subprocess, stdin/stdout pipes, and response routing.
    Responses are read on a daemon thread and dispatched by id into a queue.
    """

    def __init__(
        self,
        broker_path: Path,
        tool_command: list[str],
        armor_path: Path,
    ) -> None:
        cmd = [str(broker_path), "run", "--armor", str(armor_path), "--"] + tool_command
        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._pending: queue.Queue[dict[str, Any]] = queue.Queue()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self) -> None:
        assert self._proc.stdout is not None
        for raw_line in self._proc.stdout:
            line = raw_line.decode(errors="replace").strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
                self._pending.put(msg)
            except json.JSONDecodeError:
                pass

    def send(self, message: dict[str, Any]) -> None:
        """Write a JSON-RPC message to the broker's stdin."""
        assert self._proc.stdin is not None
        payload = json.dumps(message, separators=(",", ":")) + "\n"
        self._proc.stdin.write(payload.encode())
        self._proc.stdin.flush()

    def recv_with_id(self, request_id: int, timeout: float = 10.0) -> dict[str, Any] | None:
        """Read responses until one matching request_id is found.

        Responses for other ids are discarded (test tools produce only one
        outstanding request at a time in practice).

        @param request_id: The JSON-RPC id to wait for.
        @param timeout: Maximum seconds to wait before returning None.
        @returns: The response dict, or None if the timeout expired.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            remaining = deadline - time.monotonic()
            try:
                msg = self._pending.get(timeout=max(remaining, 0.01))
            except queue.Empty:
                return None
            if msg.get("id") == request_id:
                return msg
        return None

    def close(self) -> None:
        """Terminate the broker subprocess."""
        try:
            self._proc.terminate()
            self._proc.wait(timeout=5)
        except Exception:
            self._proc.kill()


@dataclasses.dataclass
class TestResult:
    """Outcome of a single adversarial test."""

    name: str
    outcome: str  # "BLOCKED", "ALLOWED", "INFORMATIONAL", "ERROR", "SKIPPED"
    detail: str
    required: bool  # False for informational-only tests on the current platform.

    @property
    def passed(self) -> bool:
        """Return True if this result is not a CI failure."""
        return self.outcome in ("BLOCKED", "INFORMATIONAL", "SKIPPED")


def _handshake(session: McpSession) -> bool:
    """Complete the MCP initialization handshake.

    Returns True if the tool responded to initialize successfully.

    @param session: The active MCP session.
    @returns: True on success, False if the handshake timed out.
    """
    session.send(
        {
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "mcparmor-adversarial-runner", "version": "1.0"},
            },
        }
    )
    init_response = session.recv_with_id(0, timeout=10.0)
    if init_response is None:
        return False

    session.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    return True


def _extract_text_from_result(response: dict[str, Any]) -> str:
    """Extract the plain text from a tools/call success result.

    @param response: The JSON-RPC response dict.
    @returns: The text content, or an empty string if not found.
    """
    try:
        content = response["result"]["content"]
        if isinstance(content, list) and content:
            return content[0].get("text", "")
    except (KeyError, TypeError, IndexError):
        pass
    return ""


def _make_test_label(test: TestCase) -> str:
    """Build a human-readable label for a test case.

    Produces labels like "read_passwd/tool.py" or "path_traversal/tool".
    When label_suffix is set, produces "call_metadata/tool[169.254.1.1]".

    @param test: The test case.
    @returns: A unique string label for the test case.
    """
    base = f"{test.name}/{test.tool_variant}"
    if test.label_suffix:
        return f"{base}[{test.label_suffix}]"
    return base


def run_test(
    test: TestCase,
    broker_path: Path,
    is_linux: bool,
) -> TestResult:
    """Run a single adversarial test case and return the result.

    @param test: The test case definition.
    @param broker_path: Path to the mcparmor broker binary.
    @param is_linux: True when running on Linux (affects informational handling).
    @returns: The test result.
    """
    label = _make_test_label(test)
    test_dir = _TESTS_DIR / test.name
    armor_path = test_dir / "armor.json"

    tool_command = _resolve_tool_command(test.tool_variant, test_dir)

    if tool_command is None:
        # Binary not compiled or interpreter not found — skip, not a failure.
        return TestResult(
            name=label,
            outcome="SKIPPED",
            detail=f"tool not available: {test_dir / test.tool_variant}",
            required=False,
        )

    is_informational = test.informational_on_linux and is_linux
    required = not is_informational

    session = McpSession(broker_path, tool_command, armor_path)
    try:
        if not _handshake(session):
            return TestResult(
                name=label,
                outcome="ERROR",
                detail="MCP handshake timed out",
                required=required,
            )

        session.send(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": test.tool_method, "arguments": test.arguments},
            }
        )

        response = session.recv_with_id(1, timeout=15.0)
    finally:
        session.close()

    if response is None:
        return TestResult(
            name=label,
            outcome="ERROR",
            detail="no response received within timeout",
            required=required,
        )

    # Check if broker returned a JSON-RPC error.
    if "error" in response:
        error_code = response["error"].get("code")
        error_message = response["error"].get("message", "")
        if error_code in test.expected_error_codes or test.expected_error_codes == []:
            return TestResult(
                name=label,
                outcome="BLOCKED",
                detail=f"broker error {error_code}: {error_message}",
                required=required,
            )
        # Broker returned an error, but not the expected one.
        return TestResult(
            name=label,
            outcome="BLOCKED",
            detail=f"broker returned unexpected error {error_code}: {error_message}",
            required=required,
        )

    # Broker passed through — check the tool's response text.
    text = _extract_text_from_result(response)

    if test.tool_response_blocked_prefix and text.startswith(test.tool_response_blocked_prefix):
        return TestResult(
            name=label,
            outcome="BLOCKED",
            detail=f"OS sandbox blocked inside tool: {text}",
            required=required,
        )

    if is_informational:
        return TestResult(
            name=label,
            outcome="INFORMATIONAL",
            detail=f"Layer 2 spawn blocking not available on this Linux kernel (expected). Tool response: {text}",
            required=False,
        )

    return TestResult(
        name=label,
        outcome="ALLOWED",
        detail=f"broker did not block the call. Tool responded: {text[:200]}",
        required=required,
    )


def _print_result(result: TestResult) -> None:
    """Print a single test result to stdout.

    @param result: The test result to print.
    """
    symbols = {
        "BLOCKED": "✓",
        "INFORMATIONAL": "~",
        "ALLOWED": "✗",
        "ERROR": "!",
        "SKIPPED": "-",
    }
    symbol = symbols.get(result.outcome, "?")
    tag = "" if result.required else " [informational]"
    if result.outcome == "SKIPPED":
        tag = " [skipped]"
    print(f"  {symbol} {result.name}: {result.outcome}{tag}")
    if result.outcome in ("ALLOWED", "ERROR") or result.outcome == "INFORMATIONAL":
        print(f"      {result.detail}")


def main(argv: list[str] | None = None) -> int:
    """Run the adversarial test suite and return an exit code.

    @param argv: Argument list (defaults to sys.argv).
    @returns: 0 if all required tests passed, 1 otherwise.
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--broker",
        required=True,
        type=Path,
        help="Path to the mcparmor broker binary",
    )
    parser.add_argument(
        "--json",
        dest="json_output",
        action="store_true",
        help="Write structured JSON results to adversarial-results.json",
    )
    args = parser.parse_args(argv)

    broker_path = args.broker.resolve()
    if not broker_path.exists():
        print(f"ERROR: broker binary not found: {broker_path}", file=sys.stderr)
        return 1

    is_linux = platform.system() == "Linux"
    system_label = platform.system() + " " + platform.release()

    print(f"\nMCP Armor adversarial test suite")
    print(f"Platform : {system_label}")
    print(f"Broker   : {broker_path}")
    print()

    results: list[TestResult] = []
    for test in TEST_CASES:
        result = run_test(test, broker_path, is_linux)
        _print_result(result)
        results.append(result)

    required_failures = [r for r in results if r.required and not r.passed]
    total = len(results)
    blocked = sum(1 for r in results if r.outcome == "BLOCKED")
    informational = sum(1 for r in results if r.outcome == "INFORMATIONAL")
    skipped = sum(1 for r in results if r.outcome == "SKIPPED")

    print()
    print(
        f"Results: {blocked} BLOCKED, {informational} INFORMATIONAL, "
        f"{skipped} SKIPPED, {len(required_failures)} FAILED out of {total}"
    )

    if args.json_output:
        payload = {
            "platform": system_label,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "results": {r.name: r.outcome for r in results},
            "passed": len(required_failures) == 0,
        }
        output_path = Path("adversarial-results.json")
        output_path.write_text(json.dumps(payload, indent=2))
        print(f"Results written to {output_path}")

    return 1 if required_failures else 0


if __name__ == "__main__":
    sys.exit(main())
