"""MCP Armor Python SDK — capability enforcement for MCP tool runtimes."""

from mcparmor._manifest import ArmorManifest, ManifestLoadError
from mcparmor._pool import ArmoredPool, ArmoredPoolError
from mcparmor._popen import ArmorPopenError, armor_popen
from mcparmor._process import ArmoredProcess, ArmoredProcessError
from mcparmor.testkit import ArmorErrorCode, ArmorTestHarness, ArmorTestHarnessError, ToolCallResult

__all__ = [
    "ArmorErrorCode",
    "ArmorManifest",
    "ArmorTestHarness",
    "ArmorTestHarnessError",
    "ManifestLoadError",
    "ArmoredPool",
    "ArmoredPoolError",
    "ArmoredProcess",
    "ArmoredProcessError",
    "ToolCallResult",
    "armor_popen",
    "ArmorPopenError",
]
