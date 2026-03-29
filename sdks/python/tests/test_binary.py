"""Tests for binary location logic."""

import os
import stat
from pathlib import Path
from unittest.mock import patch

import pytest

from mcparmor._binary import BinaryNotFoundError, find_binary


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_executable(path: Path) -> Path:
    """Write a stub executable file and set the executable bit."""
    path.write_text("#!/bin/sh\n")
    path.chmod(path.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
    return path


# ---------------------------------------------------------------------------
# Environment variable resolution
# ---------------------------------------------------------------------------


def test_env_override_takes_priority(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """MCPARMOR_BIN env var is used when set and the path exists."""
    binary = _make_executable(tmp_path / "mcparmor")
    monkeypatch.setenv("MCPARMOR_BIN", str(binary))

    # Patch package and PATH resolution to confirm env wins without them
    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=None):
        result = find_binary()

    assert result == binary.resolve()


def test_env_var_nonexistent_path_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    """Non-existent MCPARMOR_BIN path raises BinaryNotFoundError."""
    monkeypatch.setenv("MCPARMOR_BIN", "/this/path/does/not/exist/mcparmor")

    with pytest.raises(BinaryNotFoundError, match="does not exist"):
        find_binary()


def test_empty_env_var_is_ignored(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Empty MCPARMOR_BIN is treated as not set; falls through to next source."""
    binary = _make_executable(tmp_path / "mcparmor")
    monkeypatch.setenv("MCPARMOR_BIN", "")

    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=binary.resolve()):
        result = find_binary()

    assert result == binary.resolve()


def test_whitespace_only_env_var_is_ignored(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Whitespace-only MCPARMOR_BIN is treated as not set."""
    binary = _make_executable(tmp_path / "mcparmor")
    monkeypatch.setenv("MCPARMOR_BIN", "   ")

    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=binary.resolve()):
        result = find_binary()

    assert result == binary.resolve()


# ---------------------------------------------------------------------------
# Package-bundled binary resolution
# ---------------------------------------------------------------------------


def test_find_binary_from_package(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Bundled package binary is returned when env var is absent."""
    binary = _make_executable(tmp_path / "mcparmor")
    monkeypatch.delenv("MCPARMOR_BIN", raising=False)

    with patch("mcparmor._binary._from_package", return_value=binary.resolve()), \
         patch("mcparmor._binary._from_path", return_value=None):
        result = find_binary()

    assert result == binary.resolve()


# ---------------------------------------------------------------------------
# PATH resolution
# ---------------------------------------------------------------------------


def test_find_binary_from_path(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Binary found on PATH when env var not set and package binary absent."""
    binary = _make_executable(tmp_path / "mcparmor")
    monkeypatch.delenv("MCPARMOR_BIN", raising=False)

    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=binary.resolve()):
        result = find_binary()

    assert result == binary.resolve()


# ---------------------------------------------------------------------------
# Not found
# ---------------------------------------------------------------------------


def test_find_binary_raises_when_not_found(monkeypatch: pytest.MonkeyPatch) -> None:
    """BinaryNotFoundError raised when no binary is found by any method."""
    monkeypatch.delenv("MCPARMOR_BIN", raising=False)

    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=None):
        with pytest.raises(BinaryNotFoundError, match="mcparmor binary not found"):
            find_binary()


# ---------------------------------------------------------------------------
# Priority ordering
# ---------------------------------------------------------------------------


def test_env_takes_priority_over_package_and_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Env var wins over both package binary and PATH binary."""
    env_binary = _make_executable(tmp_path / "env_mcparmor")
    pkg_binary = _make_executable(tmp_path / "pkg_mcparmor")
    path_binary = _make_executable(tmp_path / "path_mcparmor")

    monkeypatch.setenv("MCPARMOR_BIN", str(env_binary))

    with patch("mcparmor._binary._from_package", return_value=pkg_binary.resolve()), \
         patch("mcparmor._binary._from_path", return_value=path_binary.resolve()):
        result = find_binary()

    assert result == env_binary.resolve()


def test_package_takes_priority_over_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Package binary wins over PATH binary when env var is absent."""
    pkg_binary = _make_executable(tmp_path / "pkg_mcparmor")
    path_binary = _make_executable(tmp_path / "path_mcparmor")

    monkeypatch.delenv("MCPARMOR_BIN", raising=False)

    with patch("mcparmor._binary._from_package", return_value=pkg_binary.resolve()), \
         patch("mcparmor._binary._from_path", return_value=path_binary.resolve()):
        result = find_binary()

    assert result == pkg_binary.resolve()


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


def test_non_executable_binary_from_env_raises(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Binary file without execute permission is rejected with BinaryNotFoundError."""
    binary = tmp_path / "mcparmor"
    binary.write_text("#!/bin/sh\n")
    # Deliberately do NOT set executable bit.
    binary.chmod(0o644)
    monkeypatch.setenv("MCPARMOR_BIN", str(binary))

    with pytest.raises(BinaryNotFoundError):
        find_binary()


def test_env_var_points_to_directory_raises(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """MCPARMOR_BIN pointing to a directory raises BinaryNotFoundError."""
    monkeypatch.setenv("MCPARMOR_BIN", str(tmp_path))

    with pytest.raises(BinaryNotFoundError):
        find_binary()


def test_all_sources_absent_error_message(monkeypatch: pytest.MonkeyPatch) -> None:
    """BinaryNotFoundError message mentions 'mcparmor' so users know what's missing."""
    monkeypatch.delenv("MCPARMOR_BIN", raising=False)

    with patch("mcparmor._binary._from_package", return_value=None), \
         patch("mcparmor._binary._from_path", return_value=None):
        with pytest.raises(BinaryNotFoundError) as exc_info:
            find_binary()

    assert "mcparmor" in str(exc_info.value).lower()
