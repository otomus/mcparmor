#!/usr/bin/env bash
set -euo pipefail

# Verifies MCP Armor protection is active for Cursor tools.
#
# Checks:
#   1. mcparmor is installed and on PATH
#   2. The broker reports active status for the cursor host
#   3. Secret scanning is active — a response containing a raw secret is redacted

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== MCP Armor Cursor Verification ==="
echo ""

# Check 1: mcparmor is installed
if ! command -v mcparmor &> /dev/null; then
  echo "ERROR: mcparmor not found on PATH"
  echo "Install it with: curl -fsSL https://mcp-armor.com/install.sh | bash"
  exit 1
fi

MCPARMOR_VERSION="$(mcparmor --version 2>&1 | head -1)"
echo "mcparmor installed: ${MCPARMOR_VERSION}"

# Check 2: broker status for cursor
echo ""
echo "=== Broker status ==="
mcparmor status --host cursor

# Check 3: secret scanning is active
echo ""
echo "=== Testing secret scanning ==="

ARMOR_PROFILE="${SCRIPT_DIR}/../profiles/community/github.armor.json"

if [ ! -f "${ARMOR_PROFILE}" ]; then
  echo "WARNING: ${ARMOR_PROFILE} not found — skipping secret scanning check"
  exit 0
fi

RESPONSE="$(
  echo '{"jsonrpc":"2.0","method":"ping","id":1}' \
  | timeout 5 mcparmor run --armor "${ARMOR_PROFILE}" -- \
      node -e "process.stdout.write(JSON.stringify({jsonrpc:'2.0',result:'sk-ant-api03-test12345678901234567890',id:1})+'\n')" \
  2>/dev/null || true
)"

if echo "${RESPONSE}" | grep -q "REDACTED"; then
  echo "Secret scanning: ACTIVE"
else
  echo "Secret scanning: INACTIVE"
  echo "Raw response: ${RESPONSE}"
  exit 1
fi

echo ""
echo "=== Verification complete ==="
