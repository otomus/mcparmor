"""armor_popen — subprocess.Popen wrapper with MCP Armor enforcement."""

import subprocess
from pathlib import Path
from typing import Any

from mcparmor._binary import BinaryNotFoundError, find_binary


class ArmorPopenError(OSError):
    """Raised when the mcparmor broker cannot be started."""


def armor_popen(
    command: list[str],
    *,
    armor: str | Path | None = None,
    profile: str | None = None,
    no_os_sandbox: bool = False,
    **popen_kwargs: Any,
) -> subprocess.Popen:
    """
    Spawn a command under MCP Armor enforcement.

    Wraps the given command with the mcparmor broker, which enforces the
    declared capability manifest at both the protocol level (Layer 1) and
    the OS level (Layer 2) where available.

    **Text mode** — to get decoded strings instead of raw bytes on
    stdin/stdout, pass ``text=True`` and an ``encoding`` to
    ``popen_kwargs``. These are forwarded directly to
    :class:`subprocess.Popen`::

        proc = armor_popen(
            ["python", "tool.py"],
            armor="./armor.json",
            text=True,
            encoding="utf-8",
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        )
        # proc.stdout.readline() now returns str, not bytes.

    Args:
        command: The tool command and arguments to run under armor.
        armor: Path to the armor.json manifest. If None, the broker
            searches for armor.json starting from the tool's directory.
        profile: Override the base profile declared in armor.json.
            Ignored if the manifest sets locked: true.
        no_os_sandbox: Disable OS-level sandbox (Layer 2). Protocol-level
            enforcement (Layer 1) remains active.
        **popen_kwargs: Additional keyword arguments forwarded to
            subprocess.Popen (e.g. env, cwd, stdin, stdout, text,
            encoding).

    Returns:
        A subprocess.Popen object for the armored tool process.
        The process's stdin/stdout are connected to the broker's
        stdio, which proxies to the tool.

    Raises:
        ArmorPopenError: If the mcparmor binary cannot be found or
            the broker process fails to start.
        ValueError: If command is empty.
        TypeError: If command is not a list.
    """
    if not isinstance(command, list):
        raise TypeError("command must be a non-empty list of strings.")
    if not command:
        raise ValueError("command must be a non-empty list of strings.")

    try:
        binary = find_binary()
    except BinaryNotFoundError as exc:
        raise ArmorPopenError(str(exc)) from exc

    broker_args = _build_broker_args(armor, profile, no_os_sandbox)
    broker_command = [str(binary), "run", *broker_args, "--", *command]

    try:
        return subprocess.Popen(broker_command, **popen_kwargs)
    except (OSError, FileNotFoundError) as exc:
        raise ArmorPopenError(
            f"Failed to start mcparmor broker: {exc}"
        ) from exc


def _build_broker_args(
    armor: str | Path | None,
    profile: str | None,
    no_os_sandbox: bool,
) -> list[str]:
    """
    Build the broker flag arguments for the mcparmor run sub-command.

    Args:
        armor: Path to the armor.json manifest, or None to omit the flag.
        profile: Profile name override, or None to omit the flag.
        no_os_sandbox: Whether to include the --no-os-sandbox flag.

    Returns:
        List of flag strings to insert between 'run' and '--'.
    """
    args: list[str] = []

    if armor is not None:
        args.extend(["--armor", str(armor)])

    if profile is not None:
        args.extend(["--profile", profile])

    if no_os_sandbox:
        args.append("--no-os-sandbox")

    return args
