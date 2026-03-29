"""Tests for ArmoredProcess."""

import io
import json
import subprocess
from pathlib import Path
from unittest.mock import MagicMock, patch, call

import pytest

from mcparmor._process import ArmoredProcess, ArmoredProcessError
from mcparmor._popen import ArmorPopenError

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_FAKE_RESPONSE = {"jsonrpc": "2.0", "id": 1, "result": {"ok": True}}


def _make_proc(response: dict | None = None, stdout_lines: list[str] | None = None) -> MagicMock:
    """
    Build a mock subprocess.Popen with pre-configured stdin/stdout.

    Args:
        response: Single JSON dict to encode as the stdout response line.
        stdout_lines: Explicit list of raw lines for stdout. Overrides response.
    """
    proc = MagicMock(spec=subprocess.Popen)
    proc.stdin = MagicMock()

    if stdout_lines is not None:
        raw_lines = [line.encode() if isinstance(line, str) else line for line in stdout_lines]
    elif response is not None:
        raw_lines = [(json.dumps(response) + "\n").encode()]
    else:
        raw_lines = [(json.dumps(_FAKE_RESPONSE) + "\n").encode()]

    proc.stdout = MagicMock()
    proc.stdout.readline.side_effect = raw_lines
    proc.stdout.fileno.return_value = 1  # needed by select()
    return proc


def _patch_popen(proc: MagicMock | None = None):
    """Return a context manager that patches armor_popen in the process module."""
    if proc is None:
        proc = _make_proc()
    return patch("mcparmor._process.armor_popen", return_value=proc)


def _patch_select(ready: bool = True):
    """
    Return a context manager that stubs select.select to simulate readiness.

    Args:
        ready: If True, stdout appears ready immediately. If False, simulate timeout.
    """
    stdout_mock = MagicMock()
    if ready:
        return patch("mcparmor._process.select.select", return_value=([stdout_mock], [], []))
    return patch("mcparmor._process.select.select", return_value=([], [], []))


# ---------------------------------------------------------------------------
# invoke() — single-call mode (no context manager)
# ---------------------------------------------------------------------------


def test_invoke_sends_json_rpc_line() -> None:
    """invoke() writes the JSON message followed by a newline to stdin."""
    message = {"jsonrpc": "2.0", "id": 1, "method": "run", "params": {}}
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["python", "tool.py"])
        ap.invoke(message)

    written = proc.stdin.write.call_args[0][0]
    decoded = written.decode()
    assert decoded.endswith("\n")
    assert json.loads(decoded.strip()) == message


def test_invoke_reads_and_parses_response() -> None:
    """invoke() returns the parsed JSON-RPC response dict."""
    message = {"jsonrpc": "2.0", "id": 1, "method": "run", "params": {}}
    proc = _make_proc(response=_FAKE_RESPONSE)

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["python", "tool.py"])
        result = ap.invoke(message)

    assert result == _FAKE_RESPONSE


def test_invoke_closes_process_after_single_call() -> None:
    """In single-call mode, the process is terminated after invoke()."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["python", "tool.py"])
        ap.invoke({"method": "run"})

    proc.terminate.assert_called_once()


def test_invoke_with_armor_path(tmp_path: Path) -> None:
    """armor= is forwarded to armor_popen when invoke() is called."""
    armor_file = tmp_path / "armor.json"
    armor_file.touch()
    proc = _make_proc()

    with patch("mcparmor._process.armor_popen", return_value=proc) as mock_popen, _patch_select():
        ap = ArmoredProcess(command=["python", "tool.py"], armor=armor_file)
        ap.invoke({"method": "run"})

    _, kwargs = mock_popen.call_args
    assert kwargs["armor"] == armor_file


# ---------------------------------------------------------------------------
# invoke() — timeout
# ---------------------------------------------------------------------------


def test_invoke_raises_timeout_when_select_not_ready() -> None:
    """invoke() raises TimeoutError when select reports no data within timeout."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select(ready=False):
        ap = ArmoredProcess(command=["python", "tool.py"])
        with pytest.raises(TimeoutError):
            ap.invoke({"method": "run"}, timeout=0.1)


def test_invoke_with_timeout_reads_when_ready() -> None:
    """invoke() succeeds when select reports the stream is ready within timeout."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select(ready=True):
        ap = ArmoredProcess(command=["python", "tool.py"])
        result = ap.invoke({"method": "run"}, timeout=5.0)

    assert result == _FAKE_RESPONSE


# ---------------------------------------------------------------------------
# Context manager — persistent mode
# ---------------------------------------------------------------------------


def test_context_manager_spawns_process_once() -> None:
    """Entering the context manager spawns exactly one process."""
    proc = _make_proc(stdout_lines=[
        (json.dumps(_FAKE_RESPONSE) + "\n").encode(),
        (json.dumps(_FAKE_RESPONSE) + "\n").encode(),
    ])

    with patch("mcparmor._process.armor_popen", return_value=proc) as mock_popen, _patch_select():
        with ArmoredProcess(command=["npx", "tool"]) as ap:
            ap.invoke({"method": "call1"})
            ap.invoke({"method": "call2"})

    mock_popen.assert_called_once()


def test_context_manager_allows_multiple_invocations() -> None:
    """Multiple invoke() calls in a context manager reuse the same process."""
    response1 = {"id": 1, "result": "first"}
    response2 = {"id": 2, "result": "second"}
    proc = _make_proc(stdout_lines=[
        (json.dumps(response1) + "\n").encode(),
        (json.dumps(response2) + "\n").encode(),
    ])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["npx", "tool"]) as ap:
            r1 = ap.invoke({"id": 1})
            r2 = ap.invoke({"id": 2})

    assert r1 == response1
    assert r2 == response2


def test_context_manager_closes_process_on_exit() -> None:
    """The process is terminated when the context manager exits."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["npx", "tool"]) as ap:
            ap.invoke({"method": "run"})

    proc.terminate.assert_called_once()


def test_context_manager_closes_process_on_exception() -> None:
    """The process is terminated even if an exception is raised inside the context."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        with pytest.raises(RuntimeError):
            with ArmoredProcess(command=["npx", "tool"]) as ap:
                raise RuntimeError("intentional")

    proc.terminate.assert_called_once()


# ---------------------------------------------------------------------------
# Error handling
# ---------------------------------------------------------------------------


def test_popen_failure_raises_armored_process_error() -> None:
    """ArmoredProcessError is raised when armor_popen raises ArmorPopenError."""
    with patch("mcparmor._process.armor_popen", side_effect=ArmorPopenError("no binary")):
        ap = ArmoredProcess(command=["missing_tool"])
        with pytest.raises(ArmoredProcessError, match="no binary"):
            ap.invoke({"method": "run"})


def test_armored_process_error_is_os_error_subclass() -> None:
    """ArmoredProcessError is a subclass of OSError for compatibility."""
    assert issubclass(ArmoredProcessError, OSError)


def test_invoke_outside_context_without_persistent_spawns_and_closes() -> None:
    """invoke() in single-call mode spawns and closes the process automatically."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["tool"])
        ap.invoke({"method": "run"})

    proc.terminate.assert_called_once()


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


def test_invoke_empty_response_line_raises() -> None:
    """ArmoredProcessError is raised when stdout returns an empty line."""
    proc = _make_proc(stdout_lines=[b""])

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["tool"])
        with pytest.raises(ArmoredProcessError):
            ap.invoke({"method": "run"})


def test_invoke_invalid_json_response_raises() -> None:
    """json.JSONDecodeError propagates when stdout returns malformed JSON."""
    proc = _make_proc(stdout_lines=[b"not-json\n"])

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["tool"])
        with pytest.raises(json.JSONDecodeError):
            ap.invoke({"method": "run"})


def test_close_is_idempotent() -> None:
    """Calling close() multiple times does not raise."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["tool"])
        ap.invoke({"method": "run"})
        ap.close()  # already closed after single invoke
        ap.close()  # second call must be a no-op


def test_invoke_flushes_stdin() -> None:
    """invoke() calls flush() on stdin after writing."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select():
        ap = ArmoredProcess(command=["tool"])
        ap.invoke({"method": "run"})

    proc.stdin.flush.assert_called()


def test_no_os_sandbox_forwarded_to_popen() -> None:
    """no_os_sandbox=True is forwarded to armor_popen."""
    proc = _make_proc()

    with patch("mcparmor._process.armor_popen", return_value=proc) as mock_popen, _patch_select():
        ap = ArmoredProcess(command=["tool"], no_os_sandbox=True)
        ap.invoke({"method": "run"})

    _, kwargs = mock_popen.call_args
    assert kwargs["no_os_sandbox"] is True


def test_cwd_forwarded_to_popen() -> None:
    """cwd is forwarded to armor_popen."""
    proc = _make_proc()

    with patch("mcparmor._process.armor_popen", return_value=proc) as mock_popen, _patch_select():
        ap = ArmoredProcess(command=["tool"], cwd="/some/dir")
        ap.invoke({"method": "run"})

    _, kwargs = mock_popen.call_args
    assert kwargs["cwd"] == "/some/dir"


# ---------------------------------------------------------------------------
# pid property
# ---------------------------------------------------------------------------


def test_pid_returns_none_before_spawn() -> None:
    """pid is None before the subprocess has been started."""
    ap = ArmoredProcess(command=["tool"])
    assert ap.pid is None


def test_pid_returns_process_pid_after_spawn() -> None:
    """pid returns the underlying Popen.pid after spawning."""
    proc = _make_proc()
    proc.pid = 42

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            assert ap.pid == 42


# ---------------------------------------------------------------------------
# poll()
# ---------------------------------------------------------------------------


def test_poll_returns_none_before_spawn() -> None:
    """poll() returns None when no subprocess exists."""
    ap = ArmoredProcess(command=["tool"])
    assert ap.poll() is None


def test_poll_returns_none_when_running() -> None:
    """poll() returns None for a still-running subprocess."""
    proc = _make_proc()
    proc.poll.return_value = None

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            assert ap.poll() is None


def test_poll_returns_exit_code_when_terminated() -> None:
    """poll() returns the exit code after the subprocess exits."""
    proc = _make_proc()
    proc.poll.return_value = 1

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            assert ap.poll() == 1


# ---------------------------------------------------------------------------
# is_alive()
# ---------------------------------------------------------------------------


def test_is_alive_false_before_spawn() -> None:
    """is_alive() returns False when no subprocess has been started."""
    ap = ArmoredProcess(command=["tool"])
    assert ap.is_alive() is False


def test_is_alive_true_when_running() -> None:
    """is_alive() returns True for a running subprocess."""
    proc = _make_proc()
    proc.poll.return_value = None

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            assert ap.is_alive() is True


def test_is_alive_false_when_terminated() -> None:
    """is_alive() returns False after the subprocess exits."""
    proc = _make_proc()
    proc.poll.return_value = 0

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            assert ap.is_alive() is False


# ---------------------------------------------------------------------------
# wait_ready()
# ---------------------------------------------------------------------------


def test_wait_ready_succeeds_with_ready_signal() -> None:
    """wait_ready() returns when stdout emits {"ready": true}."""
    proc = _make_proc(stdout_lines=[(json.dumps({"ready": True}) + "\n").encode()])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            ap.wait_ready()  # should not raise


def test_wait_ready_raises_on_timeout() -> None:
    """wait_ready() raises TimeoutError when select reports no data."""
    proc = _make_proc()

    with _patch_popen(proc), _patch_select(ready=False):
        with ArmoredProcess(command=["tool"]) as ap:
            with pytest.raises(TimeoutError):
                ap.wait_ready(timeout=0.01)


def test_wait_ready_raises_on_empty_stdout() -> None:
    """wait_ready() raises ArmoredProcessError if stdout closes."""
    proc = _make_proc(stdout_lines=[b""])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            with pytest.raises(ArmoredProcessError, match="closed stdout"):
                ap.wait_ready()


def test_wait_ready_raises_on_non_ready_message() -> None:
    """wait_ready() raises when the JSON line is not a ready signal."""
    proc = _make_proc(stdout_lines=[(json.dumps({"status": "ok"}) + "\n").encode()])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            with pytest.raises(ArmoredProcessError, match="Expected ready signal"):
                ap.wait_ready()


def test_wait_ready_raises_when_not_running() -> None:
    """wait_ready() raises ArmoredProcessError if process is not running."""
    ap = ArmoredProcess(command=["tool"])
    with pytest.raises(ArmoredProcessError, match="not running"):
        ap.wait_ready()


def test_ready_signal_auto_waited_in_context_manager() -> None:
    """ready_signal=True makes the context manager wait for ready automatically."""
    ready_line = (json.dumps({"ready": True}) + "\n").encode()
    response_line = (json.dumps(_FAKE_RESPONSE) + "\n").encode()
    proc = _make_proc(stdout_lines=[ready_line, response_line])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"], ready_signal=True) as ap:
            result = ap.invoke({"method": "run"})
    assert result == _FAKE_RESPONSE


def test_wait_ready_raises_on_invalid_json() -> None:
    """wait_ready() raises when stdout emits invalid JSON."""
    proc = _make_proc(stdout_lines=[b"not-json\n"])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            with pytest.raises(json.JSONDecodeError):
                ap.wait_ready()


def test_wait_ready_raises_on_ready_false() -> None:
    """wait_ready() raises when ready is explicitly false."""
    proc = _make_proc(stdout_lines=[(json.dumps({"ready": False}) + "\n").encode()])

    with _patch_popen(proc), _patch_select():
        with ArmoredProcess(command=["tool"]) as ap:
            with pytest.raises(ArmoredProcessError, match="Expected ready signal"):
                ap.wait_ready()
