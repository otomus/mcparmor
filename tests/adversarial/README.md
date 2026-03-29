# Adversarial Test Suite

The adversarial tests verify that MCP Armor's enforcement layers actually block
attacks — not that the broker correctly handles well-behaved tools, but that it
correctly handles malicious ones.

Each scenario has tools in multiple languages:

- **Go** (`tool.go`, compiled to `tool`) — the critical variant. Go produces
  statically linked binaries that make direct syscalls without going through any
  interpreter. These test Layer 2 (OS sandbox): if the OS sandbox blocks the
  syscall, the tool cannot read files, reach the network, or spawn children
  regardless of what language it uses.
- **Python** (`tool.py`) and **Node.js** (`tool.js`) — the language-agnostic
  variants. These demonstrate that Layer 1 (broker param inspection) is
  enforcement-language-independent. The broker blocks the call at the JSON-RPC
  level before the tool ever receives it.

The test runner speaks the real MCP JSON-RPC 2.0 protocol, completes the MCP
handshake, invokes the tool's declared method, and verifies the broker's response.

---

## Adversarial test matrix

The table below shows the outcome per test per platform. "Layer" indicates which
enforcement layer is responsible for the block.

| Test | Tool | macOS | Linux | Blocking layer | What the test does |
|---|---|---|---|---|---|
| `path_traversal` | `.go` | BLOCKED | BLOCKED | Layer 1 — path traversal detection | Sends a JSON-RPC call with `"path": "../../etc/passwd"` — a traversal sequence |
| `read_passwd` | `.go` | BLOCKED | BLOCKED | Layer 1 — path not in allowlist | Sends a JSON-RPC call with `"path": "/etc/passwd"` — absolute path, not in declared read list |
| `read_passwd` | `.py` / `.js` | BLOCKED | BLOCKED | Layer 1 — path not in allowlist | Same as above; confirms broker blocks regardless of tool language |
| `call_forbidden` | `.go` | BLOCKED | BLOCKED | Layer 1 — host not in network.allow | Sends a JSON-RPC call with a URL to an undeclared host |
| `call_forbidden` | `.py` / `.js` | BLOCKED | BLOCKED | Layer 1 — host not in network.allow | Same as above; confirms broker blocks regardless of tool language |
| `call_metadata` | `.go` | BLOCKED | BLOCKED | Layer 1 — deny_metadata: true | Sends `http://169.254.169.254/latest/meta-data/` — the canonical metadata IP |
| `call_metadata` | `.go` | BLOCKED | BLOCKED | Layer 1 — deny_metadata: true | Sends `http://169.254.1.1/` — confirms the full `169.254.0.0/16` CIDR is blocked, not just `.254` |
| `leak_secret` | `.go` | BLOCKED | BLOCKED | Layer 2 — output secret scanning | Tool response includes a fake AWS access key; broker redacts/blocks before forwarding |
| `leak_secret` | `.py` / `.js` | BLOCKED | BLOCKED | Layer 2 — output secret scanning | Same as above; confirms scanner runs on all tool languages |
| `spawn_child` | `.go` | BLOCKED | INFORMATIONAL | Layer 2 (macOS: Seatbelt process-exec deny) | Tool attempts to exec `/bin/sh`; `spawn: false` is declared |
| `spawn_child` | `.py` / `.js` | BLOCKED | INFORMATIONAL | Layer 2 (macOS: Seatbelt process-exec deny) | Same as above; Python `subprocess.run` and Node `spawnSync` go through the same kernel `execve` path |

### INFORMATIONAL outcome

A test marked INFORMATIONAL means the outcome on that platform is a known
limitation, not a CI failure. The test still runs and the result is reported, but
a non-BLOCKED outcome does not fail CI.

`spawn_child` is INFORMATIONAL on Linux because Layer 2 spawn blocking on Linux
is implemented via Seccomp (which denies `execve`/`execveat` syscalls). The test
result on Linux depends on the kernel version:

- Linux 3.5+: Seccomp is available and `spawn_child` should be BLOCKED.
- If the test environment has Seccomp restrictions that prevent the broker from
  installing a Seccomp filter, the block does not apply and the test outcome is
  INFORMATIONAL (recorded but not a CI failure).

On macOS, `spawn_child` is always BLOCKED — Seatbelt's `(deny process-exec)` rule
is enforced at the kernel level via `sandbox-exec`.

---

## Detailed test descriptions

### `path_traversal`

**What it tests:** Layer 1 path traversal detection.

**How it works:** The test invokes the `read_file` method with
`{"path": "../../etc/passwd"}`. The broker's param inspection detects the `../`
traversal sequence and blocks the call before forwarding it to the tool.

**Why this is Layer 1:** Path traversal is detected by inspecting the JSON-RPC
parameter value before the call is forwarded. The tool binary never receives the
call, so Layer 2 (OS sandbox) is not involved.

**Armor manifest:** `filesystem.read: ["/tmp/**"]`, `spawn: false`, no network.

**Expected broker error code:** `-32001` (capability_violation)

---

### `read_passwd`

**What it tests:** Layer 1 absolute path enforcement.

**How it works:** The test invokes `read_file` with `{"path": "/etc/passwd"}`. The
path is absolute and not matched by any pattern in `filesystem.read`. The broker
blocks the call.

**Why this is Layer 1:** Same as `path_traversal` — the block happens at the
JSON-RPC param inspection stage. The tool binary does not receive the call.

**Note on Layer 2:** On macOS, if a compiled binary attempted to open `/etc/passwd`
directly (without going through a JSON-RPC call), it would be blocked by Seatbelt
at the kernel level. On Linux 5.13+, it would be blocked by Landlock. These cases
are not exercised by this specific test (which uses JSON-RPC parameters), but are
covered by the OS-level enforcement documented in `docs/security-model.md`.

**Armor manifest:** Same as `path_traversal`.

**Expected broker error code:** `-32001` (capability_violation)

---

### `call_forbidden`

**What it tests:** Layer 1 network host enforcement.

**How it works:** The test invokes `http_fetch` with
`{"url": "https://evil.example.com/exfil"}`. The host `evil.example.com` is not
in `network.allow` (which is empty for this test). The broker blocks the call.

**Why this is Layer 1:** The broker inspects the `url` parameter value and matches
the hostname against `network.allow`. No outbound connection is attempted.

**Note on Layer 2:** On macOS, a binary that attempted a direct TCP connection to
this host (bypassing JSON-RPC entirely) would be blocked by Seatbelt's network
rules at the kernel level. On Linux, direct TCP connections to undeclared hosts are
not blocked at the kernel level — only JSON-RPC parameter inspection (Layer 1)
enforces hostname restrictions. This is the Linux hostname gap documented in
`docs/security-model.md`.

**Armor manifest:** `network.allow: []`, `deny_local: true`, `deny_metadata: true`.

**Expected broker error code:** `-32002` (network_violation)

---

### `call_metadata`

**What it tests:** Layer 1 `deny_metadata` enforcement for the cloud instance
metadata range.

**How it works:** The test invokes `http_fetch` with
`{"url": "http://169.254.169.254/latest/meta-data/"}`. The broker detects that the
URL target is in the `169.254.0.0/16` metadata range and blocks the call regardless
of `network.allow`.

**Why this is Layer 1:** `deny_metadata: true` is enforced by the broker's param
inspection layer, which checks the full `/16` CIDR range, not just the specific
`169.254.169.254` address.

**On macOS:** A compiled binary making a direct TCP connection to `169.254.169.254`
would be blocked by Seatbelt (the address falls outside the allowed network rules
under `deny default`). On Linux, a compiled binary making this connection is not
blocked at the kernel level — Layer 1 is the only protection. See
`docs/security-model.md`.

**Armor manifest:** `deny_metadata: true`, `network.allow: []`.

**Expected broker error code:** `-32002` (network_violation)

---

### `leak_secret`

**What it tests:** Layer 2 output secret scanning.

**How it works:** The test tool's `get_config` method returns a JSON response
containing a fake AWS access key (`AKIAIOSFODNN7EXAMPLE...`). With
`scan_secrets: "strict"` declared in the manifest, the broker detects the pattern
and blocks the entire response, returning a `secret_blocked` error instead of
forwarding the response to the host.

**Why this is Layer 2:** Secret scanning is applied to the tool's output (the
response coming from the tool subprocess) before it is forwarded to the MCP host.
It is output interception, not input param inspection.

**Armor manifest:** `output.scan_secrets: "strict"`.

**Expected broker error code:** `-32004` (secret_blocked)

Note: The test uses `scan_secrets: "strict"` so that the broker blocks the entire
response. With `scan_secrets: true` (redact mode), the response would be forwarded
with the secret replaced by `[REDACTED:aws_access_key]`. Both modes result in the
secret not reaching the MCP host, but `"strict"` produces a detectable broker error
code for testing purposes.

---

### `spawn_child`

**What it tests:** Layer 2 child process execution blocking.

**How it works:** The test tool's `run_command` method calls `execvp("/bin/sh", ...)`
directly (not through a JSON-RPC parameter). The broker declares `spawn: false`.

- On macOS: Seatbelt's `(deny process-exec)` rule blocks the `exec` syscall at the
  kernel level. `execvp` returns `EPERM`. The tool process catches the error and
  returns a response starting with `SPAWN_BLOCKED`. The test runner recognizes this
  prefix as a BLOCKED outcome.
- On Linux: Seccomp blocks `execve`/`execveat` syscalls on kernels where the broker
  successfully installs the filter. The outcome is BLOCKED when Seccomp is active.
  When Seccomp is unavailable (restricted container environment, very old kernel),
  the spawn may succeed and the outcome is INFORMATIONAL.

**Why there is no Layer 1 block:** `spawn_child` does not go through a JSON-RPC
parameter. The tool calls `exec()` directly. Layer 1 (param inspection) never sees
this — it only inspects JSON-RPC message content. This is why OS-level spawn
blocking (Layer 2) is necessary for a complete enforcement model.

**Armor manifest:** `spawn: false`.

**Expected outcome:** BLOCKED on macOS (Seatbelt). BLOCKED on Linux where Seccomp
is active. INFORMATIONAL on Linux where Seccomp cannot be installed.

---

## How to run the tests locally

### Prerequisites

- A built `mcparmor` binary (release or debug).
- The adversarial tool binaries compiled for your platform.

Build the broker:

```bash
cargo build --release
```

Compile the adversarial tool binaries (requires Go 1.21+):

```bash
cd tests/adversarial
go build -o path_traversal/tool ./path_traversal/
go build -o read_passwd/tool ./read_passwd/
go build -o call_forbidden/tool ./call_forbidden/
go build -o call_metadata/tool ./call_metadata/
go build -o leak_secret/tool ./leak_secret/
go build -o spawn_child/tool ./spawn_child/
cd ../..
```

Or build all at once:

```bash
cd tests/adversarial && for d in */; do go build -o "${d}tool" "./${d}" 2>/dev/null; done; cd ../..
```

### Run the test suite

```bash
python3 tests/adversarial/run_tests.py --broker ./target/release/mcparmor
```

Exit codes:
- `0` — All required tests passed (BLOCKED or INFORMATIONAL).
- `1` — One or more required tests produced ALLOWED or ERROR outcomes.

### Run with JSON output

```bash
python3 tests/adversarial/run_tests.py \
  --broker ./target/release/mcparmor \
  --json
```

Writes `adversarial-results.json` in the current directory with structured results
and a `passed` boolean.

### Run a single test manually

To run a specific test in isolation and inspect the raw MCP protocol exchange:

```bash
mcparmor run \
  --armor tests/adversarial/read_passwd/armor.json \
  -- tests/adversarial/read_passwd/tool
```

Then send MCP messages manually on stdin or use a tool like `mcp-cli`.

---

## How to add a new adversarial test

1. Create a directory under `tests/adversarial/<test-name>/`.

2. Write a Go tool in `tool.go` that implements the MCP protocol and attempts the
   attack. The tool must:
   - Respond to `initialize` with a valid capabilities response.
   - Respond to `notifications/initialized` (fire-and-forget).
   - Implement the `tools/call` method for the attack method name.
   - The attack itself — filesystem open, TCP connect, exec, etc. — must be a
     direct syscall, not routed through JSON-RPC parameters, to test Layer 2.

3. Write `armor.json` that declares what the tool is supposedly allowed to do. The
   manifest should deny the capability the attack exercises.

4. Compile the binary: `go build -o tool .`

5. Add a `TestCase` entry in `run_tests.py`:

```python
TestCase(
    name="your_test_name",
    tool_method="method_the_tool_exposes",
    arguments={},                          # args passed in tools/call.params.arguments
    expected_error_codes=[_CODE_...],      # broker error codes that mean BLOCKED
    informational_on_linux=False,          # True if Linux enforcement gap applies
    tool_response_blocked_prefix=None,     # prefix in tool text response meaning BLOCKED
),
```

6. Add the test to the adversarial test matrix table at the top of this document.

7. Run the full suite to verify the test produces the expected outcome on your
   platform.

### Adding a test for a new broker error code

Broker error codes are defined in `run_tests.py` at the top:

```python
_CODE_PATH_VIOLATION = -32001
_CODE_NETWORK_VIOLATION = -32002
_CODE_SECRET_DETECTED = -32004
```

The full error code registry is in `EXECUTION_PLAN.md` (M1 — Broker Error Codes).
Add new constants to `run_tests.py` and reference them in your `TestCase`.

---

## CI integration

The adversarial test suite runs in CI on every PR via `.github/workflows/`. The
workflow:

1. Builds the broker in release mode.
2. Compiles all adversarial tool binaries.
3. Runs `python3 tests/adversarial/run_tests.py --broker ./target/release/mcparmor --json`.
4. Fails the job if exit code is non-zero.
5. Uploads `adversarial-results.json` as a CI artifact.

The matrix runs on macOS 14 (Sonoma) and Ubuntu 22.04 (Linux 5.15 kernel). A
future matrix expansion will add Linux 6.7+ when a suitable runner is available.

---

## Why Go binaries are the critical Layer 2 tests

The compiled Go binaries are the critical adversarial tools because Go produces
statically linked (or near-statically linked) binaries that make direct syscalls
without going through an interpreter's module system.

A Python script that calls `open("/etc/passwd")` goes through:
1. Python's file I/O abstraction
2. The C standard library (`libc fopen`)
3. The kernel `open()` syscall

An instrumented Python runtime or a JSON-RPC-aware MCP framework might intercept at
step 1 or 2. A compiled Go binary bypasses both — it issues the syscall directly.
This is the correct Layer 2 test: if the OS sandbox blocks the syscall, the tool
cannot read the file regardless of what language or runtime it uses.

The Go binary tests prove that Layer 2 (OS sandbox) works, not just Layer 1
(param inspection). Without compiled binary tests, we would only know that
JSON-RPC parameter inspection works — which is Layer 1 only.

## Why Python and Node variants are also included

The Python and JS tools confirm that Layer 1 enforcement (broker param inspection)
is language-agnostic. Every adversarial scenario that is blocked at Layer 1 is
tested across all three tool languages. If the broker blocked Go tools but not
Python tools, that would be a Layer 1 bug — these variants catch it.

For Layer 2 scenarios (`spawn_child`, and for OS-sandbox tests in general), Python
and Node tools also go through the kernel `execve` syscall path. They are therefore
also subject to OS sandbox enforcement when it is active. The Python `subprocess.run`
and Node `spawnSync` calls ultimately invoke `execve` — the same syscall that
Seccomp and Seatbelt block.
