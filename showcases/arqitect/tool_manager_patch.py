"""
Arqitect ToolManager — demonstrates armor_popen integration.

This module manages the lifecycle of Arqitect tool subprocesses. It shows
the before/after pattern for adopting MCP Armor with a one-line substitution.

Before MCP Armor:
    proc = subprocess.Popen(command, stdin=PIPE, stdout=PIPE, env=env)

After MCP Armor (one-line change):
    from mcparmor import armor_popen
    proc = armor_popen(command, armor=armor_path, env=env, stdin=PIPE, stdout=PIPE)

The armor manifest path is loaded from the tool's tool.json file under the
"armor" key. If the key is absent, the tool is launched without armor (backward
compatible with tools that predate the armor field).
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import subprocess
from pathlib import Path
from subprocess import PIPE
from typing import Optional

# After: import armor_popen and ArmoredPool from mcparmor
from mcparmor import ArmoredPool, armor_popen

logger = logging.getLogger(__name__)


class ToolDescriptor:
    """
    Represents a loaded Arqitect tool definition.

    Attributes:
        name: Unique tool identifier.
        command: List of command tokens (e.g. ["python3", "tool.py"]).
        env: Extra environment variables required by the tool.
        armor_path: Absolute path to the tool's armor.json, or None if absent.
        tool_dir: Directory containing the tool's tool.json.
    """

    def __init__(
        self,
        name: str,
        command: list[str],
        env: dict[str, str],
        armor_path: Optional[str],
        tool_dir: Path,
    ) -> None:
        self.name = name
        self.command = command
        self.env = env
        self.armor_path = armor_path
        self.tool_dir = tool_dir

    @classmethod
    def from_tool_json(cls, tool_json_path: Path) -> "ToolDescriptor":
        """
        Load a ToolDescriptor from a tool.json file.

        The tool.json may optionally include an "armor" block with a "path"
        field pointing to the armor manifest. If absent, armor_path is None.

        Args:
            tool_json_path: Absolute path to the tool's tool.json file.

        Returns:
            A populated ToolDescriptor.

        Raises:
            ValueError: If required fields are missing from tool.json.
            FileNotFoundError: If the tool.json file does not exist.
        """
        with open(tool_json_path, encoding="utf-8") as f:
            data = json.load(f)

        name = data.get("name")
        command = data.get("command")
        if not name or not command:
            raise ValueError(f"tool.json missing required 'name' or 'command': {tool_json_path}")

        env = data.get("env", {})
        tool_dir = tool_json_path.parent

        armor_block = data.get("armor")
        armor_path: Optional[str] = None
        if armor_block and "path" in armor_block:
            raw_path = armor_block["path"]
            # Resolve relative paths relative to the tool directory
            resolved = (tool_dir / raw_path).resolve()
            armor_path = str(resolved)

        return cls(
            name=name,
            command=command,
            env=env,
            armor_path=armor_path,
            tool_dir=tool_dir,
        )


class ToolManager:
    """
    Manages Arqitect tool subprocesses.

    Handles launching, tracking, and stopping tool processes. When a tool
    declares an armor manifest, the subprocess is launched through armor_popen
    instead of subprocess.Popen, which enforces the declared constraints at the
    broker level before any tool code runs.
    """

    def __init__(self) -> None:
        self._running: dict[str, subprocess.Popen] = {}

    def launch(self, tool: ToolDescriptor) -> subprocess.Popen:
        """
        Launch a tool subprocess, optionally under MCP Armor enforcement.

        If the tool's descriptor includes an armor_path, the subprocess is
        started via armor_popen with that manifest. If armor_path is None,
        falls back to a bare subprocess.Popen for backward compatibility with
        tools that predate the armor field.

        Args:
            tool: The tool to launch.

        Returns:
            The subprocess handle with stdin/stdout pipes attached.

        Raises:
            RuntimeError: If the tool is already running.
            OSError: If the subprocess fails to start.
        """
        if tool.name in self._running:
            raise RuntimeError(f"Tool '{tool.name}' is already running")

        merged_env = {**os.environ, **tool.env}
        proc = self._spawn(tool, merged_env)

        self._running[tool.name] = proc
        logger.info("Launched tool '%s' (pid=%d, armor=%s)", tool.name, proc.pid, tool.armor_path)
        return proc

    def stop(self, name: str) -> None:
        """
        Stop a running tool by sending SIGTERM and waiting for exit.

        Args:
            name: Tool identifier.
        """
        proc = self._running.pop(name, None)
        if proc is None:
            return
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        logger.info("Stopped tool '%s'", name)

    def running_tools(self) -> list[str]:
        """
        Return the names of all currently running tools.

        Returns:
            List of tool name strings.
        """
        return list(self._running.keys())

    def _spawn(self, tool: ToolDescriptor, env: dict[str, str]) -> subprocess.Popen:
        """
        Spawn the tool subprocess with or without armor enforcement.

        This is the single spawn site in ToolManager. The before/after pattern
        is shown explicitly in comments so the diff is visible at code review.

        Args:
            tool: Tool descriptor with command, args, and optional armor path.
            env: Fully merged environment for the subprocess.

        Returns:
            The subprocess handle.
        """
        if tool.armor_path is not None:
            # After (one-line change): launch under MCP Armor enforcement.
            # The armor manifest at tool.armor_path is validated before the
            # process starts. If the manifest is invalid, armor_popen raises
            # before creating any subprocess.
            return armor_popen(
                tool.command,
                armor=tool.armor_path,
                env=env,
                stdin=PIPE,
                stdout=PIPE,
                stderr=PIPE,
            )

        # Before: bare subprocess.Popen with no enforcement.
        # proc = subprocess.Popen(tool.command, stdin=PIPE, stdout=PIPE, env=env)
        #
        # Kept for backward compatibility with tools that predate armor support.
        # New tools must include an armor block in their tool.json.
        logger.warning(
            "Tool '%s' has no armor manifest — running without enforcement", tool.name
        )
        return subprocess.Popen(
            tool.command,
            env=env,
            stdin=PIPE,
            stdout=PIPE,
            stderr=PIPE,
        )


class PooledToolManager:
    """
    Manages Arqitect tool subprocesses using an :class:`ArmoredPool`.

    Instead of spawning one process per tool at a time, this manager
    pre-warms a pool of processes for a single tool definition and hands
    them out on demand. Designed for workloads that maintain many concurrent
    tool instances (e.g. 50 warm processes).

    Usage::

        async with PooledToolManager(tool, pool_size=50) as manager:
            proc = await manager.acquire()
            try:
                result = proc.invoke({"method": "run", "params": {}})
            finally:
                await manager.release(proc)
    """

    def __init__(
        self,
        tool: ToolDescriptor,
        *,
        pool_size: int = 10,
        ready_signal: bool = False,
    ) -> None:
        """
        Initialise the pooled tool manager.

        Args:
            tool: Tool descriptor defining command and armor path.
            pool_size: Number of processes to pre-spawn.
            ready_signal: If True, each process waits for a ready signal.
        """
        self._tool = tool
        self._pool = ArmoredPool(
            command=tool.command,
            armor=tool.armor_path,
            size=pool_size,
            ready_signal=ready_signal,
        )

    async def __aenter__(self) -> "PooledToolManager":
        """Start the process pool."""
        await self._pool.start()
        logger.info(
            "PooledToolManager started for '%s' with %d processes",
            self._tool.name,
            self._pool.size,
        )
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: type[BaseException] | None,
    ) -> None:
        """Close the process pool."""
        await self._pool.close()
        logger.info("PooledToolManager closed for '%s'", self._tool.name)

    async def acquire(self) -> "ArmoredPool":
        """
        Acquire a process from the pool.

        Returns:
            A running ArmoredProcess ready for invoke().
        """
        return await self._pool.acquire()

    async def release(self, proc: object) -> None:
        """
        Return a process to the pool.

        Args:
            proc: The process previously obtained via acquire().
        """
        await self._pool.release(proc)  # type: ignore[arg-type]
