"""ArmoredProcess — high-level interface for invoking MCP tools under armor."""

import io
import json
import logging
import select
import subprocess
from pathlib import Path
from types import TracebackType

_logger = logging.getLogger(__name__)

from mcparmor._popen import ArmorPopenError, armor_popen

# Sentinel used to detect reads that returned no data.
_EMPTY_LINE = ""


class ArmoredProcessError(OSError):
    """Raised when an armored process cannot be started or communication fails."""


class ArmoredProcess:
    """
    High-level interface for running an MCP tool under MCP Armor enforcement.

    Supports two usage patterns:

    **Single-call pattern** — the process is spawned per :meth:`invoke` call:

    .. code-block:: python

        proc = ArmoredProcess(command=["python", "tool.py"], armor="./armor.json")
        result = proc.invoke({"method": "run", "params": {}})

    **Persistent pattern** — the process is spawned once and reused:

    .. code-block:: python

        with ArmoredProcess(["npx", "-y", "@mcp/github"], armor="./armor.json") as proc:
            r1 = proc.invoke({"method": "list_repos", "params": {}})
            r2 = proc.invoke({"method": "get_issue", "params": {"number": 42}})
    """

    def __init__(
        self,
        command: list[str],
        *,
        armor: str | Path | None = None,
        profile: str | None = None,
        no_os_sandbox: bool = False,
        cwd: str | Path | None = None,
    ) -> None:
        """
        Initialise the ArmoredProcess configuration.

        The underlying subprocess is not started here; it is started either
        by entering the context manager or on the first :meth:`invoke` call
        when used outside a context manager.

        Args:
            command: The tool command and arguments to run under armor.
            armor: Path to the armor.json manifest.
            profile: Profile name override. Ignored if the manifest is locked.
            no_os_sandbox: Disable OS-level sandbox enforcement.
            cwd: Working directory for the spawned process.
        """
        self._command = command
        self._armor = armor
        self._profile = profile
        self._no_os_sandbox = no_os_sandbox
        self._cwd = cwd
        self._process: subprocess.Popen | None = None
        self._persistent = False

    # ------------------------------------------------------------------
    # Context manager
    # ------------------------------------------------------------------

    def __enter__(self) -> "ArmoredProcess":
        """Spawn the armored process and mark it as persistent."""
        self._persistent = True
        self._spawn()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        """Terminate the armored process."""
        self.close()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def invoke(self, message: dict, *, timeout: float | None = None) -> dict:
        """
        Send a JSON-RPC message to the armored tool and return the parsed response.

        In single-call mode, a new subprocess is spawned for every call and
        terminated after the response is read. In persistent mode (context
        manager), the same process is reused across calls.

        Args:
            message: A JSON-serialisable dict representing the JSON-RPC request.
            timeout: Maximum seconds to wait for a response line. If exceeded,
                a :class:`TimeoutError` is raised.

        Returns:
            Parsed JSON-RPC response dictionary.

        Raises:
            ArmoredProcessError: If the process cannot be started or communication
                fails.
            TimeoutError: If no response is received within ``timeout`` seconds.
            json.JSONDecodeError: If the response line is not valid JSON.
        """
        if not self._persistent:
            return self._invoke_once(message, timeout=timeout)
        return self._invoke_persistent(message, timeout=timeout)

    def close(self) -> None:
        """
        Terminate the underlying subprocess if it is running.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        if self._process is None:
            return
        try:
            self._process.terminate()
            self._process.wait(timeout=5)
        except OSError as exc:
            _logger.warning("Failed to terminate armored process cleanly: %s", exc)
        finally:
            self._process = None

    # ------------------------------------------------------------------
    # Private helpers
    # ------------------------------------------------------------------

    def _spawn(self) -> None:
        """
        Start the armored subprocess with stdio pipes attached.

        Raises:
            ArmoredProcessError: If armor_popen raises ArmorPopenError.
        """
        try:
            self._process = armor_popen(
                self._command,
                armor=self._armor,
                profile=self._profile,
                no_os_sandbox=self._no_os_sandbox,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                cwd=self._cwd,
            )
        except ArmorPopenError as exc:
            raise ArmoredProcessError(str(exc)) from exc

    def _invoke_once(self, message: dict, *, timeout: float | None) -> dict:
        """Spawn, write, read, close — for single-call mode."""
        self._spawn()
        try:
            return self._send_and_receive(message, timeout=timeout)
        finally:
            self.close()

    def _invoke_persistent(self, message: dict, *, timeout: float | None) -> dict:
        """Write and read on the already-running process — for persistent mode."""
        if self._process is None:
            raise ArmoredProcessError("Process is not running; use as a context manager.")
        return self._send_and_receive(message, timeout=timeout)

    def _send_and_receive(self, message: dict, *, timeout: float | None) -> dict:
        """
        Write a JSON-RPC line to stdin and read one line from stdout.

        Args:
            message: JSON-serialisable request dictionary.
            timeout: Read timeout in seconds, or None for no timeout.

        Returns:
            Parsed response dictionary.

        Raises:
            ArmoredProcessError: If stdin/stdout are not available.
            TimeoutError: If the read exceeds ``timeout``.
        """
        proc = self._process
        if proc is None or proc.stdin is None or proc.stdout is None:
            raise ArmoredProcessError("Process stdio is unavailable.")

        line = json.dumps(message) + "\n"
        proc.stdin.write(line.encode())
        proc.stdin.flush()

        return _read_response(proc.stdout, timeout=timeout)


def _read_response(stdout: io.RawIOBase, *, timeout: float | None) -> dict:
    """
    Read one newline-delimited JSON line from stdout and parse it.

    Args:
        stdout: The process's stdout binary stream.
        timeout: Read deadline in seconds, or None for no limit.

    Returns:
        Parsed response dict.

    Raises:
        TimeoutError: If timeout is exceeded before a full line is received.
        ArmoredProcessError: If the line is empty (process closed stdout).
    """
    if timeout is not None:
        ready, _, _ = select.select([stdout], [], [], timeout)
        if not ready:
            raise TimeoutError("Timed out waiting for response from armored process.")

    raw = stdout.readline()
    text = raw.decode().strip() if isinstance(raw, bytes) else raw.strip()

    if text == _EMPTY_LINE:
        raise ArmoredProcessError("Armored process closed stdout without sending a response.")

    return json.loads(text)
