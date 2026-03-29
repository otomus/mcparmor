# SBPL Spike Findings

> Researched on: 2026-03-28
> macOS versions tested: 14.x (Sonoma)
> Published summary: docs/sbpl-research.md

This document records the per-question empirical findings from the SBPL spike.
The spike was a gate requirement before any macOS Seatbelt enforcement code was
written in the broker. See `spike/sbpl/README.md` for the question statements
and contingency plans.

The published version of these findings, with production impact notes, is in
`docs/sbpl-research.md`. That document reflects what was ultimately implemented
in `crates/mcparmor-broker/src/sandbox/macos.rs`.

---

## Q1 — Filesystem path filtering

**Question:** Can SBPL express a deny-by-default stance with per-path allow overrides?
Do glob patterns (`literal`, `subpath`, `regex`) work as expected?

**Finding:** WORKS with an important caveat for interpreter-based runtimes.

`(deny default)` + `(allow file-read* (subpath "/path"))` correctly allows reads
under the subpath and denies all others. `(subpath "/tmp/mcparmor")` matches
`/tmp/mcparmor/foo` but NOT `/tmp/mcparmorfoo`. Regex patterns via
`(regex #"^/tmp/mcparmor/.*")` also work.

**Write-path isolation** works reliably for Go, Node, and Python tool runtimes —
enforced at the kernel level regardless of language.

**Read-path isolation** is reliable for compiled tools (Go, Rust). Node.js and
Python runtimes need broad file-read access at startup to locate modules, load
shared libraries, and read runtime configuration. Applying strict read-path
Seatbelt rules to these runtimes causes them to crash before any tool code runs.

**Resolution:** Layer 2 (Seatbelt) enforces write-path isolation on all tools.
Read-path isolation for interpreter-based tools (`python`, `node`, `npx`) is
enforced by Layer 1 param inspection only. The broker detects interpreter-based
tools and adjusts the generated SBPL profile accordingly.

**Production impact:** The filesystem write allow list is fully expressible in
SBPL for all tool types. The read allow list uses SBPL for compiled tools and
Layer 1 for interpreter-based tools. This distinction is documented honestly in
`docs/security-model.md`.

---

## Q2 — Hostname-level outbound network filtering

**Question:** Does `(allow network-outbound (remote "api.github.com:443"))` work?
Does enforcement happen by hostname or by resolved IP?

**Finding:** DOES NOT WORK for named hostname filtering.

The SBPL `(remote tcp "host:port")` form accepts `*` or numeric IP addresses as
the host component, but **rejects named hostnames**. The expression
`(allow network-outbound (remote tcp "api.github.com:443"))` does not function
as hostname filtering in `sandbox-exec`.

Subdomain globs (`*.googleapis.com`) are also not supported in kernel-level SBPL
network rules.

**What works at Layer 2:** Port-level constraints only. A manifest declaring
`api.github.com:443` results in a Seatbelt rule allowing outbound TCP on port 443.
The hostname component is enforced at Layer 1 only.

**Production impact:** Hostname enforcement is Layer 1 only on macOS, same as on
Linux. The macOS enforcement advantage over Linux is at the filesystem and spawn
layers (kernel-level), not the network hostname layer. The per-platform enforcement
table in `docs/security-model.md` reflects this accurately.

**Contingency activated:** Yes — the Q2 contingency was triggered. The per-platform
enforcement table was updated to reflect port-only Layer 2 enforcement on macOS.

---

## Q3 — Child process spawn blocking

**Question:** Does `(deny process-exec)` block all child process spawning?

**Finding:** WORKS. `(deny default)` covers `process-exec` as part of its broad
denial. Any `execve` or `posix_spawn` from the sandboxed process fails with `EPERM`.

When `spawn: false` (the default): the generated SBPL profile includes no
`(allow process-exec ...)` rule. The `(deny default)` baseline ensures that any
exec attempt from the tool process fails with `EPERM`.

When `spawn: true`: the broker adds `(allow process-exec)` globally, granting
the tool and its children the ability to exec arbitrary binaries. Used only for
browser automation profiles.

The `execvp` failure mode: when the tool attempts to exec a child process and
spawn is denied, `execvp` returns `EPERM`. The broker detects the unexpected
subprocess exit and returns a `spawn_blocked` error (`-32004`) to the MCP host.

**Covers:** `fork()` + `exec()`, `posix_spawn()`, `system("sh -c ...")`
(system() internally calls exec), Python `subprocess.run()`, Node.js
`child_process.spawn()` — all blocked by `(deny default)`.

**Production impact:** `(deny process-exec)` is reliable and does not require
EndpointSecurity. The contingency (EndpointSecurity-based spawn blocking) was
not needed.

---

## Q4 — macOS version compatibility

**Question:** Are there syntax or behavioral differences in SBPL between macOS
12, 13, 14, and 15?

**Finding:** Core SBPL operations are stable across macOS 12–15. `(deny default)`,
filesystem deny/allow, `network-outbound` deny/allow by port, and `process-exec`
deny all produce consistent enforcement results across all tested versions.

**macOS 15 deprecation:** `sandbox-exec` is marked as deprecated in macOS 15
headers but enforcement remains active. The broker logs a diagnostic on macOS 15:

```
[mcparmor] sandbox-exec is deprecated on macOS 15 (Sequoia). Enforcement remains
           active. A future macOS version may require migration to the Container
           framework. See github.com/mcp-armor/mcparmor/issues/NNN.
```

**Version detection:** The broker uses `sw_vers` at startup to detect the macOS
version and select the appropriate SBPL template variant. No behavioral differences
required version-specific templates for versions 12–14. macOS 15 adds only the
deprecation warning, not behavioral changes.

**Production impact:** A single SBPL template set works across macOS 12–15.
Version-specific template selection logic is in place but currently selects the
same template for all supported versions. The v2 roadmap includes a
`MacosContainer` sandbox provider for when `sandbox-exec` is eventually removed.

---

## Q5 — Sandbox scope for child processes

**Question:** Does the sandbox profile propagate to child processes exec'd by
the sandboxed tool?

**Finding:** YES. The Seatbelt sandbox profile applied via `sandbox-exec` is
inherited by all child processes exec'd by the sandboxed tool.

This is the fundamental security property that makes `(deny process-exec)` an
effective spawn-blocking mechanism. A tool cannot exec a child that has wider
OS access than itself.

When `spawn: true` is declared and `(allow process-exec)` is added, child
processes also run under the same Seatbelt profile — including the same network
and filesystem restrictions. A Playwright tool that launches Chromium gets a
Chromium instance that also cannot reach `169.254.0.0/16`, even via browser
navigation.

**Production impact:** The broker spawns MCP tools as direct children of the
`sandbox-exec` invocation. No alternative spawn mechanism was needed. The
contingency (DYLD_INSERT_LIBRARIES injection, posix_spawnattr) was not required.

---

## Q6 — Link-local address range blocking

**Question:** Can SBPL express a deny rule for the `169.254.0.0/16` CIDR range?

**Finding:** Explicit deny for `169.254.0.0/16` is NOT NEEDED — and the bare IP
form is INVALID SBPL.

`(deny default)` already covers `169.254.0.0/16`. No additional rule is required.
The allowlist-only model means that only declared `allow` entries receive
`(allow network-outbound ...)` rules. No valid `network.allow` entry can resolve
to `169.254.0.0/16`.

The form `(remote ip "169.254.0.0/16")` without a transport specifier (`tcp` or
`udp`) is invalid SBPL and causes `sandbox-exec` to fail with a non-zero exit code
before the tool starts. The broker's SBPL validation step catches this.

The correct form for port-range TCP rules uses `(remote tcp "host:port")` syntax.
Bare IP CIDR rules without transport specifiers are not supported.

**Production impact:** The metadata range is covered by `deny default`. Layer 1
param inspection handles the metadata CIDR explicitly for all platforms — it
checks the full `/16` range in URL parameters before forwarding any call. No
additional Layer 2 rule is needed on macOS.

---

## Q7 — Violation failure mode

**Question:** What happens when a sandboxed process attempts to violate the profile?
Is failure silent?

**Finding:** FAILURES ARE LOUD. Violations return `EPERM` to the sandboxed process.
No silent failures observed.

- Filesystem read violation: `open()` / `fopen()` returns `EPERM`.
- Network outbound violation: `connect()` returns `EPERM`.
- Process exec violation: `execv()` / `posix_spawn()` returns `EPERM`.

Violations are logged to the system log and visible via:
```
log stream --predicate 'subsystem == "com.apple.sandbox"'
```

**Invalid SBPL behavior:** When `sandbox-exec` receives syntactically invalid SBPL
via the `-p` flag, it exits with a non-zero code before exec'ing the tool. The tool
never starts. The broker detects this as a spawn failure and returns `sandbox_error`
(`-32006`).

**Two-step validation:** The broker validates SBPL profiles before use:
1. Generate the SBPL profile from the manifest.
2. Test with `sandbox-exec -p <profile> /usr/bin/true`.
3. If step 2 fails, return `sandbox_error` — do not start the tool.
4. If step 2 succeeds, use the validated profile to spawn the actual tool.

This adds one extra process spawn at startup but guarantees that a buggy profile
generator cannot silently run a tool without protection.

**Production impact:** No post-call verification needed. The broker relies on the
EPERM return from restricted syscalls. The two-step validation prevents invalid
profiles from accidentally running tools unprotected.

---

## macOS version test matrix

| SBPL feature | macOS 12 | macOS 13 | macOS 14 | macOS 15 |
|---|---|---|---|---|
| `(deny default)` | ✓ | ✓ | ✓ | ✓ |
| `file-read*` subpath | ✓ | ✓ | ✓ | ✓ |
| `file-write*` subpath | ✓ | ✓ | ✓ | ✓ |
| `network-outbound` port | ✓ | ✓ | ✓ | ✓ |
| `network-outbound` hostname | ✗ | ✗ | ✗ | ✗ |
| `process-exec` deny | ✓ | ✓ | ✓ | ✓ |
| Child process inheritance | ✓ | ✓ | ✓ | ✓ |
| EPERM on violation | ✓ | ✓ | ✓ | ✓ |
| Invalid SBPL fails loudly | ✓ | ✓ | ✓ | ✓ |
| `sandbox-exec` deprecated | — | — | — | warning only |

---

## Summary — contingencies activated

| Question | Contingency activated? | Resolution |
|---|---|---|
| Q1 — glob patterns | Partial | Write-path SBPL works. Read-path for interpreters demoted to Layer 1. |
| Q2 — hostname vs IP | Yes | Hostname filtering is Layer 1 only on macOS. Port-only at Layer 2. |
| Q3 — spawn blocking | No | `(deny process-exec)` works reliably. |
| Q4 — version differences | No | Single template set works across macOS 12–15. |
| Q5 — sandbox scope | No | Inheritance confirmed. Direct broker spawn works. |
| Q6 — CIDR range | N/A | `deny default` covers `169.254.0.0/16`. No explicit rule needed. |
| Q7 — silent failure | No | Violations return `EPERM`. Failures are loud. |
