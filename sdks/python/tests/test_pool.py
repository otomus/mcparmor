"""Tests for ArmoredPool."""

import asyncio
import json
from unittest.mock import MagicMock, patch

import pytest

from mcparmor._pool import ArmoredPool, ArmoredPoolError
from mcparmor._process import ArmoredProcess


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_mock_proc(alive: bool = True, pid: int = 1234) -> MagicMock:
    """
    Build a mock subprocess.Popen that simulates a running or dead process.

    Args:
        alive: If True, poll() returns None (running). Otherwise returns 0.
        pid: The fake PID to assign.
    """
    proc = MagicMock()
    proc.pid = pid
    proc.poll.return_value = None if alive else 0
    proc.stdin = MagicMock()
    proc.stdout = MagicMock()
    proc.stdout.fileno.return_value = 1
    return proc


def _patch_spawn():
    """Patch armor_popen to return a mock process."""
    return patch(
        "mcparmor._process.armor_popen",
        return_value=_make_mock_proc(),
    )


# ---------------------------------------------------------------------------
# Constructor validation
# ---------------------------------------------------------------------------


def test_pool_rejects_zero_size() -> None:
    """ArmoredPool raises ValueError when size is 0."""
    with pytest.raises(ValueError, match="at least 1"):
        ArmoredPool(command=["tool"], size=0)


def test_pool_rejects_negative_size() -> None:
    """ArmoredPool raises ValueError when size is negative."""
    with pytest.raises(ValueError, match="at least 1"):
        ArmoredPool(command=["tool"], size=-5)


def test_pool_accepts_size_one() -> None:
    """ArmoredPool accepts size=1 without raising."""
    pool = ArmoredPool(command=["tool"], size=1)
    assert pool.size == 1


# ---------------------------------------------------------------------------
# start()
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_start_spawns_processes() -> None:
    """start() creates the configured number of processes."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=3)
        await pool.start()
        assert pool.available == 3
        await pool.close()


@pytest.mark.asyncio
async def test_start_twice_raises() -> None:
    """start() raises ArmoredPoolError if called twice."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        with pytest.raises(ArmoredPoolError, match="already started"):
            await pool.start()
        await pool.close()


@pytest.mark.asyncio
async def test_start_after_close_raises() -> None:
    """start() raises ArmoredPoolError if pool was already closed."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        await pool.close()
        with pytest.raises(ArmoredPoolError, match="closed"):
            await pool.start()


# ---------------------------------------------------------------------------
# acquire() / release()
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_acquire_returns_process() -> None:
    """acquire() returns an ArmoredProcess."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        proc = await pool.acquire()
        assert isinstance(proc, ArmoredProcess)
        assert pool.available == 0
        await pool.release(proc)
        assert pool.available == 1
        await pool.close()


@pytest.mark.asyncio
async def test_acquire_before_start_raises() -> None:
    """acquire() raises ArmoredPoolError if pool is not started."""
    pool = ArmoredPool(command=["tool"], size=1)
    with pytest.raises(ArmoredPoolError, match="not started"):
        await pool.acquire()


@pytest.mark.asyncio
async def test_acquire_after_close_raises() -> None:
    """acquire() raises ArmoredPoolError if pool is closed."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        await pool.close()
        with pytest.raises(ArmoredPoolError, match="closed"):
            await pool.acquire()


# ---------------------------------------------------------------------------
# close()
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_close_is_idempotent() -> None:
    """close() can be called multiple times without error."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        await pool.close()
        await pool.close()  # no-op


# ---------------------------------------------------------------------------
# Context manager
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_context_manager() -> None:
    """ArmoredPool works as an async context manager."""
    with _patch_spawn():
        async with ArmoredPool(command=["tool"], size=2) as pool:
            assert pool.available == 2
            proc = await pool.acquire()
            assert pool.available == 1
            await pool.release(proc)
            assert pool.available == 2


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_release_to_closed_pool_closes_process() -> None:
    """Releasing a process after pool closure closes the process."""
    with _patch_spawn():
        pool = ArmoredPool(command=["tool"], size=1)
        await pool.start()
        proc = await pool.acquire()
        await pool.close()
        # Release after close — should not raise
        await pool.release(proc)


@pytest.mark.asyncio
async def test_size_property() -> None:
    """The size property reflects the configured pool size."""
    pool = ArmoredPool(command=["tool"], size=7)
    assert pool.size == 7


@pytest.mark.asyncio
async def test_default_size() -> None:
    """Default pool size is 4."""
    pool = ArmoredPool(command=["tool"])
    assert pool.size == 4
