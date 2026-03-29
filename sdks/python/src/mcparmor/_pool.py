"""ArmoredPool — pre-spawned pool of ArmoredProcess instances."""

import asyncio
import logging
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from types import TracebackType

from mcparmor._process import ArmoredProcess

_logger = logging.getLogger(__name__)


class ArmoredPoolError(OSError):
    """Raised when a pool operation fails."""


class ArmoredPool:
    """
    Manages a pool of warm :class:`ArmoredProcess` instances.

    Pre-spawns *size* processes at :meth:`start` time and hands them out
    via :meth:`acquire` / :meth:`release`. Designed for workloads that
    maintain many concurrent tool processes (e.g. Arqitect's 50-process pool).

    Usage as an async context manager::

        async with ArmoredPool(
            command=["python", "tool.py"],
            armor="./armor.json",
            size=10,
        ) as pool:
            proc = await pool.acquire()
            try:
                result = proc.invoke({"method": "run", "params": {}})
            finally:
                await pool.release(proc)

    Args:
        command: The tool command and arguments for each process.
        armor: Path to the armor.json manifest.
        size: Number of processes to pre-spawn.
        ready_signal: If True, each process waits for a ``{"ready": true}``
            line from stdout before becoming available.
        profile: Profile name override.
        no_os_sandbox: Disable OS-level sandbox enforcement.
        cwd: Working directory for spawned processes.
    """

    def __init__(
        self,
        command: list[str],
        *,
        armor: str | Path | None = None,
        size: int = 4,
        ready_signal: bool = False,
        profile: str | None = None,
        no_os_sandbox: bool = False,
        cwd: str | Path | None = None,
    ) -> None:
        if size < 1:
            raise ValueError("Pool size must be at least 1.")

        self._command = command
        self._armor = armor
        self._size = size
        self._ready_signal = ready_signal
        self._profile = profile
        self._no_os_sandbox = no_os_sandbox
        self._cwd = cwd

        self._available: asyncio.Queue[ArmoredProcess] = asyncio.Queue(maxsize=size)
        self._all: list[ArmoredProcess] = []
        self._lock = asyncio.Lock()
        self._started = False
        self._closed = False

    # ------------------------------------------------------------------
    # Async context manager
    # ------------------------------------------------------------------

    async def __aenter__(self) -> "ArmoredPool":
        """Start the pool and return it."""
        await self.start()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        """Close all pooled processes."""
        await self.close()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    async def start(self) -> None:
        """
        Pre-spawn all processes in the pool.

        Each process is started as a persistent :class:`ArmoredProcess`
        (context-manager style). If *ready_signal* was set, each process
        waits for the ready handshake before joining the available queue.

        Raises:
            ArmoredPoolError: If the pool has already been started or closed.
        """
        async with self._lock:
            if self._closed:
                raise ArmoredPoolError("Pool has been closed.")
            if self._started:
                raise ArmoredPoolError("Pool is already started.")

            for _ in range(self._size):
                proc = self._create_process()
                proc._persistent = True
                proc._spawn()
                if self._ready_signal:
                    proc.wait_ready()
                self._all.append(proc)
                self._available.put_nowait(proc)

            self._started = True
            _logger.info("ArmoredPool started with %d processes", self._size)

    async def acquire(self) -> ArmoredProcess:
        """
        Acquire an available process from the pool.

        Blocks until a process becomes available via :meth:`release`.

        Returns:
            A running :class:`ArmoredProcess` ready for :meth:`~ArmoredProcess.invoke`.

        Raises:
            ArmoredPoolError: If the pool is not started or has been closed.
        """
        if not self._started:
            raise ArmoredPoolError("Pool is not started. Call start() first.")
        if self._closed:
            raise ArmoredPoolError("Pool has been closed.")
        return await self._available.get()

    async def release(self, proc: ArmoredProcess) -> None:
        """
        Return a process to the pool.

        If the process has died (``is_alive()`` is False), it is replaced
        with a freshly spawned one.

        Args:
            proc: The process previously obtained via :meth:`acquire`.

        Raises:
            ArmoredPoolError: If the pool has been closed.
        """
        if self._closed:
            proc.close()
            return

        if not proc.is_alive():
            _logger.warning("Replacing dead process (pid=%s) in pool", proc.pid)
            proc.close()
            replacement = self._create_process()
            replacement._persistent = True
            replacement._spawn()
            if self._ready_signal:
                replacement.wait_ready()

            async with self._lock:
                self._all = [p if p is not proc else replacement for p in self._all]
            proc = replacement

        await self._available.put(proc)

    async def close(self) -> None:
        """
        Close all processes in the pool.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        async with self._lock:
            if self._closed:
                return
            self._closed = True

        for proc in self._all:
            proc.close()
        self._all.clear()
        _logger.info("ArmoredPool closed")

    @property
    def size(self) -> int:
        """Return the configured pool size."""
        return self._size

    @property
    def available(self) -> int:
        """Return the number of currently available processes."""
        return self._available.qsize()

    # ------------------------------------------------------------------
    # Private helpers
    # ------------------------------------------------------------------

    def _create_process(self) -> ArmoredProcess:
        """Create a new ArmoredProcess with this pool's configuration."""
        return ArmoredProcess(
            command=self._command,
            armor=self._armor,
            profile=self._profile,
            no_os_sandbox=self._no_os_sandbox,
            cwd=self._cwd,
        )
