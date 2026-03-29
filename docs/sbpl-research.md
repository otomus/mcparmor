# macOS Seatbelt SBPL Research Findings

This document records the findings from the SBPL spike conducted before M1 broker
development. SBPL (Sandbox Profile Language) is the language used to generate macOS
Seatbelt profiles passed to `sandbox-exec`. Apple does not document SBPL publicly;
this spike was a gate requirement before any broker code that depended on Seatbelt
was written.

The spike addressed 7 questions that had to be answered before the macOS sandbox
provider could be designed. The results here are what the code actually implements
in `crates/mcparmor-broker/src/sandbox/macos.rs`.

---

## Q1: Filesystem isolation — read vs. write

**Question:** Can SBPL express both read and write path isolation? Does write-path
isolation work reliably for Go, Node, and Python tool runtimes?

**Finding:** SBPL write-path isolation works and is implemented. Filesystem write
restrictions are enforced at the kernel level on macOS via Seatbelt regardless of
the tool's language or runtime.

**File read isolation is deferred to Layer 1 for interpreter-based tools.** Go and
Rust binaries (statically linked or with well-known dylib paths) can have read
restrictions applied at the kernel level. However, Node.js and Python runtimes need
broad file-read access at startup to locate modules, load shared libraries, and read
configuration from user home directories. Applying strict read-path Seatbelt rules
to these runtimes causes them to fail at initialization before any tool code runs.

The resolution: Layer 2 (Seatbelt) enforces write-path isolation on all tools.
Read-path isolation on interpreter-based tools (`python`, `node`, `npx`) is enforced
by Layer 1 param inspection only. The broker detects interpreter-based tools and
adjusts the generated SBPL profile accordingly.

**What this means for users:** A compiled Go or Rust tool that declares
`filesystem.read: ["/tmp/mcparmor/*"]` is restricted at the kernel level — it
genuinely cannot open `/etc/passwd`. A Python tool with the same declaration has its
reads enforced by Layer 1 (the broker blocks JSON-RPC calls that reference
undeclared paths), but a Python script that calls `open("/etc/passwd")` directly
without going through JSON-RPC parameters would not be blocked on the filesystem
read by Layer 2 on macOS. The `read_passwd` adversarial test covers this distinction.

---

## Q2: Hostname filtering — `network-outbound` syntax and behavior

**Question:** Does `(allow network-outbound (remote hostname "*.googleapis.com" port 443))`
work? What is the correct SBPL expression for `allow: ["api.github.com:443"]` and
port wildcards like `localhost:*`? Does subdomain glob expansion work in Seatbelt?

**Finding:** The SBPL `(remote tcp "host:port")` form accepts `*` or `localhost` as
the host component, but **rejects named hostnames**. The expression
`(allow network-outbound (remote tcp "api.github.com:443"))` does not work as
intended for external hostname filtering in the `sandbox-exec` interface exposed by
`sandbox-exec`.

**What the implementation does instead:** Named hostname filtering is enforced at
Layer 1 (protocol param inspection) on macOS in the same way as on Linux. The Seatbelt
profile uses port-level constraints where the `allow` list contains specific ports,
and hostname enforcement at the broker level handles the hostname component.

**Port-only enforcement at Layer 2:** The broker translates the declared `network.allow`
list into port-level Seatbelt rules. A manifest declaring `api.github.com:443` results
in a Seatbelt rule that allows outbound TCP on port 443. The specific hostname
restriction to `api.github.com` is enforced by Layer 1 only.

**Hostname enforcement is Layer 1 only on macOS as well as Linux.** The macOS
enforcement advantage over Linux is at the filesystem and spawn layers (kernel-level),
not the network hostname layer. The per-platform enforcement table in
`docs/security-model.md` reflects this accurately.

**SBPL template for port-level network rules:**

```scheme
; Allow outbound TCP on specific ports from network.allow
(allow network-outbound
  (remote tcp "*:443")
  (remote tcp "*:80"))

; Always deny link-local metadata range (deny_metadata: true)
(deny network-outbound
  (remote ip "169.254.0.0/16"))

; Deny loopback when deny_local: true
(deny network-outbound
  (remote ip "127.0.0.0/8")
  (remote ip "::1"))
```

---

## Q3: Spawn blocking — `process-exec` deny syntax

**Question:** What is the correct SBPL for blocking child process spawning? Does
`(deny process-exec)` work? How does `spawn: true` manifest in the generated profile?

**Finding:** `(deny default)` in the baseline Seatbelt profile covers `process-exec`
as part of its broad denial. Explicit `(allow process-exec (literal "/abs/path"))`
expressions allow only the specific tool binary path.

When `spawn: false` (the default): the generated SBPL profile does not include any
`(allow process-exec ...)` rule. The `(deny default)` baseline ensures that any
`execve` from the tool process fails with `EPERM`. The tool binary itself is allowed
to exec via the initial `sandbox-exec` invocation, which is how `sandbox-exec` works —
the exec of the target binary is permitted by the sandbox framework before the profile
takes effect.

When `spawn: true`: the broker adds `(allow process-exec)` globally to the generated
profile, granting the tool and its children the ability to exec arbitrary binaries.
This is used only for browser automation profiles (`profile: "browser"` with
`spawn: true`) where the tool must launch a headless browser subprocess.

**SBPL spawn blocking template:**

```scheme
; spawn: false — no additional process-exec rules; deny default covers it

; spawn: true — explicitly allow process execution
(allow process-exec)
```

**`execvp` failure mode:** When `spawn: false` and the tool attempts to exec a child
process, `execvp` returns `EPERM`. If the tool does not handle this error, it exits
with a non-zero code. The broker detects the unexpected subprocess exit and returns
a `spawn_blocked` error (`-32004`) to the MCP host.

---

## Q4: macOS version compatibility

**Question:** Do the SBPL rules work identically across macOS 12 (Monterey), 13
(Ventura), 14 (Sonoma), and 15 (Sequoia)? Does `sandbox-exec` on macOS 15 still
enforce, or does the deprecation mean weakened enforcement?

**Finding:** `sandbox-exec` ships with all macOS 12+ releases. The `(deny default)`
baseline is stable across all tested versions (12–15). Core SBPL operations
(filesystem deny, network deny by range, process-exec deny) produce consistent
enforcement results across all versions.

**macOS 15 deprecation:** `sandbox-exec` is marked as deprecated in macOS 15
headers but enforcement remains active. Deprecated in Apple's toolchain means
"may be removed in a future OS version", not "enforcement is reduced now". The
broker logs a diagnostic on macOS 15:

```
[mcparmor] sandbox-exec is deprecated on macOS 15 (Sequoia). Enforcement remains
           active. A future macOS version may require migration to the Container
           framework. See github.com/mcp-armor/mcparmor/issues/NNN.
```

The v2 roadmap includes a `MacosContainer` sandbox provider as a replacement when
the Apple Container framework stabilizes. The `SandboxProvider` trait was designed
with this migration in mind.

**macOS version detection:** The broker uses `sw_vers` at startup to detect the
macOS version and select the appropriate SBPL template variant. Enforcement
differences across versions are handled by feature detection, not version hardcoding.

---

## Q5: Sandbox scope — child process inheritance

**Question:** Is the Seatbelt sandbox profile inherited by child processes that the
tool exec()s?

**Finding:** Yes. The Seatbelt sandbox profile applied via `sandbox-exec` is inherited
by all child processes exec'd by the sandboxed tool. This is the fundamental security
property that makes `(deny process-exec)` effective as a spawn-blocking mechanism and
why `spawn: true` requires an explicit profile addition rather than just being
silently bypassed.

A tool running under a Seatbelt profile with `(deny process-exec)` cannot exec a
child process that then has full OS access. If `spawn: true` is declared and
`(allow process-exec)` is added to the profile, any child process the tool spawns
also runs under the same Seatbelt profile — including the same network and filesystem
restrictions.

**Practical implication for browser automation:** A Playwright tool running under
`profile: "browser"` with `spawn: true` launches a Chromium subprocess. That
Chromium subprocess also runs under the same Seatbelt profile — including the
`deny_metadata: true` restriction on `169.254.0.0/16`. This means browser
automation tools cannot reach cloud metadata services via browser navigation, even
with `spawn: true` and broad network access.

---

## Q6: Link-local metadata range blocking

**Question:** Can Seatbelt block `169.254.0.0/16` as a CIDR range, or only specific
addresses? Is `(remote ip "X.X.X.X")` without a port valid SBPL?

**Finding:** The `169.254.0.0/16` range is covered by `(deny default)` — no explicit
deny rule is needed for it. When `deny_metadata: true` is set, the broker generates
no additional explicit rule for the metadata range, because `(deny default)` already
blocks all network access and only the `allow` entries are permitted.

**`(remote ip "X.X.X.X")` without port is invalid SBPL.** The correct form for
IP-based network rules requires the `remote tcp` or `remote udp` qualifier. The
form `(remote ip "169.254.0.0/16")` without a transport specifier is not valid and
causes `sandbox-exec` to fail. The broker uses port-range TCP rules rather than
bare IP rules.

**CIDR blocking implementation:** Rather than generating an explicit deny rule for
the metadata range, the broker relies on the allowlist-only model. The `(deny default)`
baseline denies all network access. Only the declared `allow` entries receive
`(allow network-outbound ...)` rules. No entry in a valid `network.allow` list can
resolve to `169.254.0.0/16` — the validator rejects any such entry if somehow
encountered, and no hostname resolves to this range in normal operation.

The Layer 1 param inspection handles the metadata CIDR explicitly for all platforms —
it checks the full `/16` range in URL parameters before forwarding any call.

---

## Q7: Violation failure mode

**Question:** What happens when `sandbox-exec` receives invalid SBPL — does it fail
loudly with an error code, or silently run the child unprotected? What is the
`execvp` return when exec is denied?

**Finding:** `execvp` returns `EPERM` (error code 1) when the Seatbelt profile
denies `process-exec`. The tool binary fails at startup if the exec itself is denied
by the profile. This is not the normal case — `sandbox-exec` permits the exec of the
target binary — but applies to any child process that the sandboxed tool attempts to
create when `spawn: false`.

**Invalid SBPL behavior:** When `sandbox-exec` receives a syntactically invalid SBPL
profile via the `-p` flag, it fails with a non-zero exit code before exec'ing the
tool. The tool never starts. The broker detects this as a spawn failure and returns
a `sandbox_error` JSON-RPC error (`-32006`).

This is the correct and safe behavior: invalid SBPL fails loudly rather than silently
running the child without protection. The broker includes a mandatory SBPL validation
step before spawning any tool:

1. Generate the SBPL profile from the manifest.
2. Test the profile with `sandbox-exec -p <profile> /usr/bin/true`.
3. If step 2 fails, return `sandbox_error` and do not start the tool.
4. If step 2 succeeds, use the validated profile to spawn the actual tool.

This two-step approach adds a small startup overhead (one extra process spawn) but
guarantees that a buggy profile generator cannot silently run a tool without protection.

---

## SBPL templates

The following templates are the actual profiles generated by
`crates/mcparmor-broker/src/sandbox/macos.rs` for each built-in profile preset.

### `strict` profile

```scheme
(version 1)
(deny default)
(allow file-read*
  (literal "/usr/lib")
  (literal "/usr/lib/libSystem.B.dylib")
  (subpath "/usr/lib/system")
  (subpath "/System/Library/Frameworks")
  (subpath "/System/Library/PrivateFrameworks"))
(deny network-outbound)
(deny process-exec)
```

The minimal allow list for system libraries is required for the tool binary to
load at all — even a statically linked Go binary needs access to the dynamic linker
on macOS.

### `sandboxed` profile (with example paths and ports)

```scheme
(version 1)
(deny default)
(allow file-read*
  (literal "/usr/lib")
  (subpath "/usr/lib/system")
  (subpath "/System/Library/Frameworks")
  (subpath "/System/Library/PrivateFrameworks")
  (subpath "/tmp/mcparmor"))
(allow file-write*
  (subpath "/tmp/mcparmor"))
(allow network-outbound
  (remote tcp "*:443"))
(deny network-outbound
  (remote tcp "127.0.0.0/8:*")
  (remote tcp "::1:*"))
(deny process-exec)
```

### `network` profile

```scheme
(version 1)
(deny default)
(allow file-read*
  (literal "/usr/lib")
  (subpath "/usr/lib/system")
  (subpath "/System/Library/Frameworks")
  (subpath "/System/Library/PrivateFrameworks"))
(allow network-outbound
  (remote tcp "*:443")
  (remote tcp "*:80"))
(deny network-outbound
  (remote tcp "127.0.0.0/8:*")
  (remote tcp "::1:*"))
(deny process-exec)
```

### `browser` profile

```scheme
(version 1)
(deny default)
(allow file-read*
  (literal "/usr/lib")
  (subpath "/usr/lib/system")
  (subpath "/System/Library/Frameworks")
  (subpath "/System/Library/PrivateFrameworks")
  (subpath "/tmp/mcparmor"))
(allow file-write*
  (subpath "/tmp/mcparmor"))
(allow network-outbound
  (remote tcp "*:443")
  (remote tcp "*:80")
  (remote tcp "127.0.0.1:*")
  (remote tcp "::1:*"))
(allow process-exec)
```

`deny_local: false` for the browser profile adds the localhost TCP rules. No
metadata range deny is needed — the default deny covers `169.254.0.0/16`.

---

## Known limitations

**Hostname filtering is not kernel-enforced on macOS in the current implementation.**
The initial plan assumed that SBPL could express `(allow network-outbound (remote
tcp "api.github.com:443"))` — filtering by hostname at the kernel level. The spike
found that this does not work as expected in `sandbox-exec`. Hostname enforcement
falls back to Layer 1 (param inspection) on macOS, the same as on Linux.

**Subdomain globs (`*.googleapis.com`) are not supported in the kernel-level rules.**
For the same reason — SBPL hostname rules in the TCP `remote` form do not support
wildcard expansion. Wildcard entries in `network.allow` are effective at Layer 1
only.

**`sandbox-exec` is deprecated on macOS 15.** Enforcement remains active as of the
time of writing, but the v2 roadmap includes migration to the Apple Container
framework. Community users on macOS 15 should watch the v2 milestone.

**System library allow list must be maintained.** The minimal set of filesystem
read rules required for binaries to load at all on macOS must be kept up to date as
macOS evolves. The broker tests this on each CI run by verifying that a simple tool
binary can start under the generated strict profile.

**Interpreter-based tools (Python, Node) receive reduced Layer 2 filesystem
read protection.** As documented in Q1, the runtime startup requirements of
interpreters make kernel-level read-path restriction impractical without extensive
per-runtime tuning. Layer 1 enforces read paths for these tools. This is documented
honestly in `docs/security-model.md`.
