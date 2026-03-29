# SBPL Spike

**Status:** `[ ] IN PROGRESS — gate for M1 macOS Seatbelt code`

---

## What is this spike?

This is a pre-M1 validation gate. Before any macOS Seatbelt enforcement code is written in the broker, we must confirm that SBPL (Sandbox Profile Language) can express every constraint the armor manifest requires. This spike answers that question empirically, on real hardware, across macOS versions 12 through 15.

No macOS Seatbelt production code should be written until this spike is complete and `findings.md` is populated.

---

## Why a spike?

SBPL is undocumented and Apple-internal. Apple has never published a stable grammar or a compatibility table. Assumptions that prove false mid-implementation would require a broker rewrite — not a patch.

The risk profile is asymmetric: running the spike costs a few days; discovering a wrong assumption after M1 ships costs weeks and breaks the macOS security model guarantee. The spike resolves that uncertainty before a line of production code is written.

---

## The 7 questions to answer

### Q1 — Filesystem path filtering with defaults and overrides

Can SBPL express a deny-by-default stance with per-path allow overrides?

```scheme
(deny file-read-data)
(allow file-read-data (subpath "/home/user/project"))
```

Do glob patterns (`(literal ...)`, `(subpath ...)`, `(regex ...)`) work as expected? Does `(subpath "/foo")` correctly match `/foo/bar/baz` but not `/foobar`?

**Failure mode if no:** Use path prefix matching instead of glob expressions; document the glob limitation in `docs/security-model.md`.

---

### Q2 — Hostname-level outbound network filtering

Can SBPL express hostname-based outbound network restrictions?

```scheme
(allow network-outbound (remote "api.github.com:443"))
```

Critically: does enforcement happen by hostname (re-resolved at connection time) or by IP address at rule-compilation time? If it resolves to IP at compile time, network filtering is degraded — a CDN rotation would silently bypass the rule.

**Failure mode if no (IP-based):** Hostname filtering is demoted to Layer 1 only on macOS. The per-platform enforcement table in `docs/security-model.md` is updated to reflect this. Layer 2 on macOS becomes port-only for network rules.

---

### Q3 — Child process spawn blocking

Does `(deny process-exec)` block all child process spawning, including via `posix_spawn`? Does it also block interpreter invocations such as `sh -c "..."`?

Test cases:
- `fork()` + `exec()`
- `posix_spawn()`
- `system("sh -c ...")`
- Python `subprocess.run()`
- Node.js `child_process.spawn()`

**Failure mode if no:** Spawn blocking may require an EndpointSecurity-based approach on macOS. Spike EndpointSecurity as a contingency; update the broker architecture if needed.

---

### Q4 — macOS version compatibility

Are there syntax or behavioral differences in SBPL between macOS 12, 13, 14, and 15? Specifically:
- Do the same rule forms compile and evaluate identically?
- Are there rules present on 15 that do not exist on 12?
- Does Apple enforce any deprecation warnings or silent ignores on older forms?

**Failure mode if yes (differences exist):** Generate version-specific SBPL templates and detect the macOS version at runtime in the broker. The template selection logic becomes part of the production code path.

---

### Q5 — Sandbox scope for child processes

Does `sandbox-exec -p <profile>` sandbox the subprocess correctly, or does the sandbox only apply to the `sandbox-exec` process itself and not its children?

This matters because the broker spawns the MCP tool as a child. If the sandbox does not propagate, Seatbelt enforcement is void.

**Failure mode if no:** The broker must use a different spawn mechanism to apply the sandbox. Options: inject via `DYLD_INSERT_LIBRARIES`, use `posix_spawnattr` with sandbox attributes, or apply the profile to the broker process before exec. The spike findings unblock the design decision.

---

### Q6 — Link-local address range blocking

Can SBPL express a deny rule for the `169.254.0.0/16` (APIPA / AWS metadata) CIDR range?

```scheme
(deny network-outbound (remote ip "169.254.0.0/16"))
```

Or must individual addresses be enumerated? The metadata service endpoint (`169.254.169.254`) is the primary target, but the full range matters for completeness.

**Failure mode if no:** Block individual high-value addresses (`169.254.169.254`, `169.254.170.2`) in SBPL and verify coverage. Document the limitation.

---

### Q7 — Violation failure mode

What happens when a sandboxed process attempts to violate the profile?

- Does the syscall return `EPERM`?
- Does the process receive `SIGKILL` or `SIGABRT`?
- Is the violation logged to the system log (`log stream --predicate 'subsystem == "com.apple.sandbox"'`)?
- Is the failure silent (returns success but no effect)?

Silent failure is the worst outcome. If violations are silent, the broker must add post-call verification to detect them.

**Failure mode if silent:** Add verification steps after sensitive syscalls. Evaluate whether silent-failure mode can be detected and surfaced to the user.

---

## Deliverables

| Path | Description |
|---|---|
| `spike/sbpl/profiles/` | Working SBPL template files for each armor manifest field |
| `spike/sbpl/test-tools/` | Small C programs that test each SBPL capability in isolation |
| `spike/sbpl/findings.md` | Per-question findings with macOS version matrix |
| `docs/sbpl-research.md` | Published version of findings, linked from the main docs |

Each test tool in `spike/sbpl/test-tools/` should be a single-purpose C program that attempts exactly one potentially-blocked action (read a forbidden path, connect to a forbidden host, spawn a child). The test is run inside a `sandbox-exec` invocation with a profile from `spike/sbpl/profiles/` and the exit code or error output confirms the enforcement result.

---

## Contingency summary

| Question | If the answer is "no" or "yes (unexpected)" | Impact |
|---|---|---|
| Q1 — glob patterns | Use prefix matching; document glob limitation | Low — minor expressiveness loss |
| Q2 — hostname vs IP | Hostname filtering is Layer 1 only on macOS | Medium — update platform enforcement table |
| Q3 — spawn blocking | Spike EndpointSecurity as alternative | High — broker architecture decision |
| Q4 — version differences | Generate version-specific templates; runtime detection | Medium — added broker complexity |
| Q5 — sandbox scope | Change broker spawn mechanism | High — fundamental broker design |
| Q6 — CIDR range | Enumerate individual addresses; document coverage | Low — minor coverage gap |
| Q7 — silent failure | Add post-call verification; surface to user | High — enforcement correctness |
