"""Expected SHA256 checksums for bundled mcparmor binaries.

This file is generated at wheel-build time by the cibuildwheel pipeline.
Each entry maps a platform tag (matching the wheel filename) to the expected
SHA256 hex digest of the bundled binary. At import time, ``_binary.py``
verifies the bundled binary against this table before returning its path.

If this file contains no entries (``BINARY_CHECKSUMS = {}``), checksum
verification is skipped — this is the case for development installs built
from source without cibuildwheel.
"""

# Populated by CI during wheel build. Format:
#   "<platform_tag>": "<sha256_hex>"
# Example:
#   "macosx_14_0_arm64": "abcdef0123456789...",
#   "manylinux_2_28_x86_64": "fedcba9876543210...",
BINARY_CHECKSUMS: dict[str, str] = {}
