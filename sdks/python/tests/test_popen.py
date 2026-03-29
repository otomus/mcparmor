"""Tests for armor_popen."""

import subprocess
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from mcparmor import ArmorPopenError, armor_popen
from mcparmor._binary import BinaryNotFoundError

# Sentinel path used wherever a real binary is needed in mocked tests.
_FAKE_BINARY = Path("/usr/local/bin/mcparmor")


def _patch_binary(binary: Path = _FAKE_BINARY):
    """Return a context manager that stubs find_binary with the given path."""
    return patch("mcparmor._popen.find_binary", return_value=binary)


def _patch_popen(proc: MagicMock | None = None):
    """Return a context manager that stubs subprocess.Popen."""
    if proc is None:
        proc = MagicMock(spec=subprocess.Popen)
    return patch("mcparmor._popen.subprocess.Popen", return_value=proc)


# ---------------------------------------------------------------------------
# Input validation
# ---------------------------------------------------------------------------


def test_raises_on_empty_command() -> None:
    """armor_popen raises ValueError for an empty command list."""
    with pytest.raises(ValueError, match="non-empty"):
        armor_popen([])


def test_raises_on_none_command() -> None:
    """armor_popen raises TypeError or ValueError for None command."""
    with pytest.raises((TypeError, ValueError)):
        armor_popen(None)  # type: ignore[arg-type]


def test_raises_on_non_list_command() -> None:
    """armor_popen raises TypeError or ValueError when command is not a list."""
    with pytest.raises((TypeError, ValueError)):
        armor_popen("node /path/to/tool")  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# Broker command construction
# ---------------------------------------------------------------------------


def test_wraps_command_with_broker() -> None:
    """The broker binary and 'run' sub-command are prepended to the tool command."""
    tool_command = ["node", "/path/to/tool"]

    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(tool_command)

    called_command = mock_popen.call_args[0][0]
    assert called_command[0] == str(_FAKE_BINARY)
    assert called_command[1] == "run"


def test_separator_between_broker_args_and_command() -> None:
    """'--' separator appears between broker flags and the tool command."""
    tool_command = ["node", "/path/to/tool"]

    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(tool_command)

    called_command = mock_popen.call_args[0][0]
    assert "--" in called_command
    separator_index = called_command.index("--")
    # Tool command must follow the separator
    assert called_command[separator_index + 1 :] == tool_command


def test_armor_flag_included_when_path_given(tmp_path: Path) -> None:
    """--armor flag and the manifest path are added when armor= is specified."""
    manifest = tmp_path / "armor.json"
    manifest.touch()

    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], armor=manifest)

    called_command = mock_popen.call_args[0][0]
    assert "--armor" in called_command
    armor_index = called_command.index("--armor")
    assert called_command[armor_index + 1] == str(manifest)


def test_armor_flag_omitted_when_none() -> None:
    """No --armor flag appears when armor=None."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], armor=None)

    called_command = mock_popen.call_args[0][0]
    assert "--armor" not in called_command


def test_armor_accepts_string_path() -> None:
    """armor= accepts a plain string as well as a Path object."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], armor="/some/armor.json")

    called_command = mock_popen.call_args[0][0]
    assert "--armor" in called_command
    armor_index = called_command.index("--armor")
    assert called_command[armor_index + 1] == "/some/armor.json"


def test_profile_flag_included_when_given() -> None:
    """--profile flag and its value are added when profile= is specified."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], profile="strict")

    called_command = mock_popen.call_args[0][0]
    assert "--profile" in called_command
    profile_index = called_command.index("--profile")
    assert called_command[profile_index + 1] == "strict"


def test_profile_flag_omitted_when_none() -> None:
    """No --profile flag appears when profile=None."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], profile=None)

    called_command = mock_popen.call_args[0][0]
    assert "--profile" not in called_command


def test_no_os_sandbox_flag_included() -> None:
    """--no-os-sandbox flag is added when no_os_sandbox=True."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], no_os_sandbox=True)

    called_command = mock_popen.call_args[0][0]
    assert "--no-os-sandbox" in called_command


def test_no_os_sandbox_flag_omitted_by_default() -> None:
    """--no-os-sandbox flag is absent when no_os_sandbox=False (default)."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"])

    called_command = mock_popen.call_args[0][0]
    assert "--no-os-sandbox" not in called_command


def test_all_flags_combined() -> None:
    """All optional flags appear together in the correct order."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(
            ["node", "tool.js"],
            armor="/armor.json",
            profile="strict",
            no_os_sandbox=True,
        )

    called_command = mock_popen.call_args[0][0]
    separator_index = called_command.index("--")
    broker_flags = called_command[2:separator_index]  # after binary + 'run'

    assert "--armor" in broker_flags
    assert "--profile" in broker_flags
    assert "--no-os-sandbox" in broker_flags


# ---------------------------------------------------------------------------
# Popen forwarding
# ---------------------------------------------------------------------------


def test_popen_kwargs_forwarded() -> None:
    """Additional kwargs (env, cwd, etc.) are forwarded to subprocess.Popen."""
    import os

    extra_env = {"MY_VAR": "hello"}

    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], env=extra_env, cwd="/tmp")

    _, kwargs = mock_popen.call_args
    assert kwargs["env"] == extra_env
    assert kwargs["cwd"] == "/tmp"


def test_returns_popen_object() -> None:
    """armor_popen returns the subprocess.Popen instance."""
    expected_proc = MagicMock(spec=subprocess.Popen)

    with _patch_binary(), _patch_popen(expected_proc):
        result = armor_popen(["node", "tool.js"])

    assert result is expected_proc


# ---------------------------------------------------------------------------
# Error handling
# ---------------------------------------------------------------------------


def test_binary_not_found_raises_armor_error() -> None:
    """ArmorPopenError is raised when find_binary raises BinaryNotFoundError."""
    with patch(
        "mcparmor._popen.find_binary",
        side_effect=BinaryNotFoundError("not found"),
    ):
        with pytest.raises(ArmorPopenError, match="not found"):
            armor_popen(["node", "tool.js"])


def test_popen_oserror_raises_armor_error() -> None:
    """ArmorPopenError is raised when subprocess.Popen raises OSError."""
    with _patch_binary(), patch(
        "mcparmor._popen.subprocess.Popen",
        side_effect=OSError("permission denied"),
    ):
        with pytest.raises(ArmorPopenError, match="permission denied"):
            armor_popen(["node", "tool.js"])


def test_popen_filenotfound_raises_armor_error() -> None:
    """ArmorPopenError is raised when subprocess.Popen raises FileNotFoundError."""
    with _patch_binary(), patch(
        "mcparmor._popen.subprocess.Popen",
        side_effect=FileNotFoundError("binary gone"),
    ):
        with pytest.raises(ArmorPopenError):
            armor_popen(["node", "tool.js"])


def test_armor_error_is_os_error_subclass() -> None:
    """ArmorPopenError is a subclass of OSError for compatibility."""
    assert issubclass(ArmorPopenError, OSError)


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


def test_raises_on_dict_command() -> None:
    """armor_popen raises TypeError when command is a dict."""
    with pytest.raises((TypeError, ValueError)):
        armor_popen({"cmd": "node"})  # type: ignore[arg-type]


def test_raises_on_int_command() -> None:
    """armor_popen raises TypeError when command is an integer."""
    with pytest.raises((TypeError, ValueError)):
        armor_popen(42)  # type: ignore[arg-type]


def test_command_list_with_spaces_in_args() -> None:
    """Commands whose arguments contain spaces are passed through verbatim."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "/path with spaces/tool.js", "--arg=value with space"])

    called_command = mock_popen.call_args[0][0]
    assert "/path with spaces/tool.js" in called_command
    assert "--arg=value with space" in called_command


def test_command_with_unicode_paths() -> None:
    """Commands with unicode characters in args are preserved unchanged."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "/tmp/tëst/tøøl.js"])

    called_command = mock_popen.call_args[0][0]
    assert "/tmp/tëst/tøøl.js" in called_command


def test_single_element_command() -> None:
    """Single-element command list (no extra args) is wrapped correctly."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node"])

    called_command = mock_popen.call_args[0][0]
    separator_index = called_command.index("--")
    assert called_command[separator_index + 1 :] == ["node"]


def test_armor_flag_with_string_path_preserved() -> None:
    """armor= as a plain string is passed as-is without Path conversion."""
    with _patch_binary(), _patch_popen() as mock_popen:
        armor_popen(["node", "tool.js"], armor="/tmp/special chars!/armor.json")

    called_command = mock_popen.call_args[0][0]
    armor_index = called_command.index("--armor")
    assert called_command[armor_index + 1] == "/tmp/special chars!/armor.json"
