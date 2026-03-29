"""
currency-converter — Arqitect MCP tool that converts between currencies.

Reads newline-delimited JSON-RPC 2.0 requests from stdin and writes responses
to stdout. The armor manifest (armor.json) in this directory declares that this
tool may only reach api.frankfurter.app:443. All other network access,
filesystem writes, and subprocess spawning are denied by the MCP Armor broker.

Supported methods:
    tools/list  — returns the list of available tools
    tools/call  — dispatches to the convert_currency tool handler

Tools:
    convert_currency — converts an amount between two ISO 4217 currency codes
"""

from __future__ import annotations

import json
import sys
import urllib.error
import urllib.request
from typing import Any

FRANKFURTER_BASE_URL = "https://api.frankfurter.app/latest"


def fetch_exchange_rate(from_currency: str, to_currency: str) -> float:
    """
    Fetch the live exchange rate between two currencies from Frankfurter.

    Args:
        from_currency: ISO 4217 source currency code (e.g. "USD").
        to_currency: ISO 4217 target currency code (e.g. "EUR").

    Returns:
        The exchange rate as a float (amount of to_currency per 1 from_currency).

    Raises:
        ValueError: If the currency pair is not supported.
        OSError: If the API request fails.
    """
    url = f"{FRANKFURTER_BASE_URL}?from={from_currency}&to={to_currency}"
    try:
        with urllib.request.urlopen(url, timeout=10) as response:
            data = json.loads(response.read())
    except urllib.error.HTTPError as err:
        raise ValueError(f"Currency API error {err.code}: {err.reason}") from err

    rates = data.get("rates", {})
    if to_currency not in rates:
        raise ValueError(f"Unsupported currency pair: {from_currency} -> {to_currency}")
    return float(rates[to_currency])


def convert_currency(amount: float, from_currency: str, to_currency: str) -> dict[str, Any]:
    """
    Convert an amount from one currency to another using live exchange rates.

    Args:
        amount: Amount to convert (must be positive).
        from_currency: ISO 4217 source currency code.
        to_currency: ISO 4217 target currency code.

    Returns:
        MCP tools/call result dict with a text content item.

    Raises:
        ValueError: If the amount is not positive or the currencies are invalid.
    """
    if amount <= 0:
        raise ValueError("Amount must be positive")
    if not from_currency.strip() or not to_currency.strip():
        raise ValueError("Currency codes must not be empty")

    rate = fetch_exchange_rate(from_currency.upper(), to_currency.upper())
    converted = round(amount * rate, 2)

    return {
        "content": [
            {
                "type": "text",
                "text": (
                    f"{amount} {from_currency.upper()} = {converted} {to_currency.upper()} "
                    f"(rate: 1 {from_currency.upper()} = {rate} {to_currency.upper()})"
                ),
            }
        ]
    }


def handle_tools_list() -> dict[str, Any]:
    """
    Return the MCP tools/list result for this tool.

    Returns:
        Dict with a 'tools' key listing all available tools and their schemas.
    """
    return {
        "tools": [
            {
                "name": "convert_currency",
                "description": "Convert an amount from one currency to another using live exchange rates.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "amount": {"type": "number", "description": "Amount to convert"},
                        "from": {"type": "string", "description": "ISO 4217 source currency code (e.g. USD)"},
                        "to": {"type": "string", "description": "ISO 4217 target currency code (e.g. EUR)"},
                    },
                    "required": ["amount", "from", "to"],
                },
            }
        ]
    }


def handle_tools_call(params: dict[str, Any]) -> dict[str, Any]:
    """
    Dispatch a tools/call request to the appropriate tool handler.

    Args:
        params: JSON-RPC params dict with 'name' and 'arguments' keys.

    Returns:
        MCP tools/call result dict.

    Raises:
        ValueError: If the tool name is unknown or arguments are invalid.
    """
    tool_name = params.get("name")
    arguments = params.get("arguments", {})

    if tool_name != "convert_currency":
        raise ValueError(f"Unknown tool: {tool_name}")

    amount = arguments.get("amount")
    from_currency = arguments.get("from")
    to_currency = arguments.get("to")

    if amount is None or from_currency is None or to_currency is None:
        raise ValueError("convert_currency requires 'amount', 'from', and 'to' arguments")

    return convert_currency(float(amount), str(from_currency), str(to_currency))


def process_request(request: dict[str, Any]) -> dict[str, Any]:
    """
    Process a single parsed JSON-RPC 2.0 request and return a response dict.

    Args:
        request: Parsed JSON-RPC request object.

    Returns:
        JSON-RPC 2.0 response dict (success or error).
    """
    request_id = request.get("id")
    method = request.get("method")
    params = request.get("params", {})

    try:
        if method == "tools/list":
            result = handle_tools_list()
        elif method == "tools/call":
            result = handle_tools_call(params)
        else:
            return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32601, "message": f"Method not found: {method}"}}
        return {"jsonrpc": "2.0", "id": request_id, "result": result}
    except ValueError as err:
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32000, "message": str(err)}}
    except OSError as err:
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32000, "message": f"Network error: {err}"}}


def main() -> None:
    """Read newline-delimited JSON-RPC requests from stdin and write responses to stdout."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            response = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}}
            sys.stdout.write(json.dumps(response) + "\n")
            sys.stdout.flush()
            continue

        response = process_request(request)
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
