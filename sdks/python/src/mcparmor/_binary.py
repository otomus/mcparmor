"""Binary location resolution for the mcparmor broker."""

import hashlib
import os
import platform
import shutil
import sys
from importlib.resources import files
from pathlib import Path

from ._checksums import BINARY_CHECKSUMS


class BinaryNotFoundError(RuntimeError):
    """Raised when the mcparmor binary cannot be located."""


class BinaryChecksumError(RuntimeError):
    """Raised when the bundled binary fails SHA256 verification."""


_BINARY_NAME = "mcparmor.exe" if sys.platform == "win32" else "mcparmor"
_ENV_VAR = "MCPARMOR_BIN"


def find_binary() -> Path:
    """
    Locate the mcparmor binary using a priority search.

    Search order:
    1. ``MCPARMOR_BIN`` environment variable (no checksum check — caller's responsibility)
    2. Bundled binary in the installed wheel (SHA256 verified against ``_checksums.py``)
    3. Binary on PATH (no checksum check — comes from the user's system)

    Returns:
        Absolute path to the mcparmor binary.

    Raises:
        BinaryNotFoundError: If the binary cannot be found by any method.
        BinaryChecksumError: If the bundled binary fails SHA256 verification.
    """
    candidate = _from_env() or _from_package() or _from_path()
    if candidate is None:
        raise BinaryNotFoundError(
            "mcparmor binary not found. Set MCPARMOR_BIN, install a binary wheel, "
            "or ensure mcparmor is on PATH."
        )
    return candidate


def _from_env() -> Path | None:
    """
    Resolve the binary path from the MCPARMOR_BIN environment variable.

    Returns:
        Absolute path if the env var is set to a non-empty, existing path;
        None otherwise.

    Raises:
        BinaryNotFoundError: If the env var is set but the path does not exist.
    """
    raw = os.environ.get(_ENV_VAR, "").strip()
    if not raw:
        return None

    candidate = Path(raw)
    if not candidate.exists():
        raise BinaryNotFoundError(
            f"{_ENV_VAR} is set to '{raw}' but that path does not exist."
        )
    if not candidate.is_file():
        raise BinaryNotFoundError(
            f"{_ENV_VAR} is set to '{raw}' but that path is not a file."
        )
    if not os.access(candidate, os.X_OK):
        raise BinaryNotFoundError(
            f"{_ENV_VAR} is set to '{raw}' but that file is not executable."
        )
    return candidate.resolve()


def _from_package() -> Path | None:
    """
    Resolve the binary bundled inside the installed wheel and verify its SHA256.

    When ``_checksums.BINARY_CHECKSUMS`` is populated (wheel builds), the
    binary's SHA256 is checked before returning the path. Development installs
    (empty checksums table) skip verification.

    Returns:
        Absolute path to the bundled binary if present, executable, and
        passing the checksum check; None if the binary is not bundled.

    Raises:
        BinaryChecksumError: If the binary exists but fails the SHA256 check.
    """
    try:
        resource = files("mcparmor").joinpath(f"bin/{_BINARY_NAME}")
        candidate = Path(str(resource))
        if not (candidate.exists() and os.access(candidate, os.X_OK)):
            return None
        candidate = candidate.resolve()
        _verify_checksum(candidate)
        return candidate
    except (TypeError, ModuleNotFoundError):
        return None


def _verify_checksum(binary_path: Path) -> None:
    """
    Verify a binary's SHA256 against the expected checksum for this platform.

    Skipped when ``BINARY_CHECKSUMS`` is empty (development installs).

    Args:
        binary_path: Absolute path to the binary to verify.

    Raises:
        BinaryChecksumError: If a checksum is registered for this platform
            but the binary's digest does not match.
    """
    if not BINARY_CHECKSUMS:
        return

    platform_tag = _current_platform_tag()
    expected = BINARY_CHECKSUMS.get(platform_tag)
    if expected is None:
        # No checksum registered for this platform — skip verification.
        return

    actual = _sha256(binary_path)
    if actual != expected:
        raise BinaryChecksumError(
            f"mcparmor binary at {binary_path} failed SHA256 verification.\n"
            f"  Expected: {expected}\n"
            f"  Actual  : {actual}\n"
            "The binary may have been tampered with. Reinstall the package to fix this."
        )


def _sha256(path: Path) -> str:
    """Return the lowercase hex SHA256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _current_platform_tag() -> str:
    """
    Return a platform tag matching the wheel naming convention.

    Used to look up the expected checksum in ``BINARY_CHECKSUMS``.
    """
    system = platform.system().lower()
    machine = platform.machine().lower()

    # Normalise machine names to wheel convention.
    machine_map = {
        "x86_64": "x86_64",
        "amd64": "x86_64",
        "aarch64": "aarch64",
        "arm64": "arm64",  # macOS convention
    }
    machine = machine_map.get(machine, machine)

    if system == "darwin":
        major, minor = platform.mac_ver()[0].split(".")[:2]
        return f"macosx_{major}_{minor}_{machine}"
    if system == "linux":
        return f"manylinux_2_28_{machine}"
    if system == "windows":
        return f"win_{machine}"
    return f"{system}_{machine}"


def _from_path() -> Path | None:
    """
    Resolve the binary from the system PATH.

    Returns:
        Absolute path to the binary if found on PATH; None otherwise.
    """
    found = shutil.which("mcparmor")
    if found is None:
        return None
    return Path(found).resolve()
