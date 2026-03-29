# Security Policy

MCP Armor's purpose is capability enforcement for MCP tools. A vulnerability in
MCP Armor's enforcement — one that lets a tool exceed its declared capabilities,
leak secrets, or escape the OS sandbox — is a serious issue that warrants a private
disclosure process.

This document describes what is in scope, how to report, and what to expect in
response.

---

## Supported versions

| Version | Supported |
|---|---|
| 1.x | Active — security patches issued |
| < 1.0 | No patches |

---

## Responsible disclosure

**Do not open a public GitHub issue for security vulnerabilities.**

Send vulnerability reports to: **security@mcp-armor.com**

You may also use GitHub's private security advisory mechanism: Security tab →
"Report a vulnerability."

Encrypt your report using our PGP key if the content is sensitive. The key is
available at `https://mcp-armor.com/.well-known/security.asc`.

Include in your report:

- A description of the vulnerability and the enforcement layer it affects (Layer 1,
  Layer 2, or both).
- The MCP Armor version and platform (OS + kernel version) where you reproduced it.
- Reproduction steps — ideally a minimal armor manifest and tool binary or script
  that demonstrates the bypass.
- The impact: what capability restriction is defeated, and under what conditions.

You do not need a working exploit to report. A well-described theoretical bypass
with supporting evidence is sufficient to initiate a response.

---

## Response SLA

| Severity | Acknowledgment | Patch target |
|---|---|---|
| Critical — enforcement bypass allowing arbitrary file read/write, network exfiltration to undeclared hosts, or OS sandbox escape | 48 hours | 14 days |
| High — partial bypass, redaction evasion, or secret scanner defeat | 48 hours | 30 days |
| Medium — information leak via audit log, timing side-channel | 48 hours | 60 days |
| Low — documentation gap, unclear behavior | 48 hours | Next release |

"Patch target" means a fix merged to `main` and a release published. For critical
issues, a patch release is cut immediately rather than waiting for the next scheduled
release.

We will acknowledge receipt within 48 hours of a report sent to
`security@mcp-armor.com`. If you do not receive acknowledgment within 72 hours,
follow up at the same address.

We will keep you informed throughout the process and credit you in the release notes
unless you prefer to remain anonymous.

---

## Scope: what is in scope

Vulnerabilities in MCP Armor's enforcement belong in scope if they allow a tool to
exceed its declared capabilities — to do something that a correct reading of the
manifest says should be blocked.

### Broker enforcement bypass (Layer 1)

- A tool receives a JSON-RPC call with a path or URL that should have been blocked
  by param inspection.
- A secret in a tool response is not detected or is not redacted/blocked as declared.
- A tool environment contains variables that were not declared in `env.allow`.
- A timeout is not enforced, allowing a tool to run indefinitely.
- A response exceeding `max_size_kb` is not truncated.
- An `armor.json` manifest that fails schema validation is loaded without error,
  causing enforcement gaps.

### OS sandbox escape (Layer 2)

- A tool running under the generated Seatbelt profile on macOS successfully reads
  or writes a filesystem path not declared in the manifest at the kernel level.
- A tool running under Landlock on Linux successfully reads or writes a filesystem
  path not declared in the manifest at the kernel level (on a kernel version where
  Landlock is supposed to be active).
- A tool running under Seccomp successfully exec's a child process when `spawn: false`
  is declared (on a kernel version where Seccomp is active).
- The SBPL profile generator produces a profile that is syntactically valid but
  does not enforce what the manifest declares.

### Secret leakage

- A secret in a tool response that matches a known scanner pattern passes through
  undetected when `scan_secrets: true` or `"strict"` is declared.
- Pattern evasion: a trivially modified secret string consistently evades detection.

### Supply chain / profile registry integrity

- A profile distributed via `mcparmor profiles update` does not match the SHA-256
  checksum in the signed manifest, but the broker accepts it.
- The signed manifest's signature can be forged or bypassed.
- The broker binary is tampered and the checksum verification passes.

---

## Scope: what is out of scope

The following are not vulnerabilities in MCP Armor. They are documented design
decisions. Please do not report these — they will be closed as "known limitation."

### Layer 1-only gaps on older kernels

On Linux kernels older than 5.13, Landlock filesystem isolation is not available. A
compiled Go binary can read `/etc/passwd` on these kernels. This is a documented
limitation, not a bug.

On all Linux versions, hostname-level network enforcement is Layer 1 only. A compiled
binary making direct `connect()` syscalls to an undeclared IP address is not blocked
at the kernel level on Linux.

These are limitations of what the Linux kernel exposes. Reports about them are
acknowledged as "known limitation" and will not receive a patch unless a new kernel
primitive enables enforcement.

### Malicious `armor.json` authored by the tool

A tool author who ships an overly permissive manifest has not exploited a
vulnerability — they have declared too many capabilities. The mitigation is the
community profile review process.

### HTTP/remote MCP tools

MCP Armor only protects tools it wraps as subprocesses. Tools that run as remote
HTTP or SSE servers are not wrapped, and MCP Armor provides no enforcement for them.

### The AI model or MCP host

Prompt injection, jailbreaking, and model-level attacks are not MCP Armor's concern.

### Kernel CVEs

If a kernel security module (Landlock, Seccomp, Seatbelt) has a vulnerability that
allows privilege escalation, that is a kernel bug. Patch your OS. MCP Armor inherits
whatever enforcement the kernel provides.

### `locked: true` bypass by a privileged operator

`locked: true` is a cooperative signal to compliant runtimes. A user who chooses not
to use MCP Armor at all cannot be stopped by it. `locked: true` is not a kernel-level
control.

### Windows enforcement gaps

MCP Armor v1 provides Layer 1 (protocol enforcement) only on Windows. No OS sandbox
is implemented. This is documented. Reports about Windows tools bypassing kernel-level
enforcement will be acknowledged as "known limitation — v3 roadmap."

---

## Known limitations

These limitations are documented in `docs/security-model.md`. They are not
vulnerabilities, but they are tracked on the roadmap.

| Limitation | Affected platforms | Mitigation |
|---|---|---|
| Hostname-level network enforcement is Layer 1 only | All Linux versions | Use macOS for kernel-level hostname enforcement; use network namespace isolation at the infrastructure level |
| No filesystem isolation for compiled binaries on Linux < 5.13 | Linux < 5.13 | Upgrade to Linux 5.13+, or use containers with overlayfs |
| No TCP port isolation on Linux 5.13–6.6 | Linux 5.13–6.6 | Upgrade to Linux 6.7+, or use network namespace isolation |
| No OS sandbox on Windows | Windows | Use Linux or macOS; Windows AppContainer planned for v3 |
| Interpreter runtimes (Python, Node) require relaxed read-path rules on macOS | macOS, Python/Node tools | Layer 1 enforces read-path restrictions for interpreter tools; filesystem write-path is kernel-enforced |
| `sandbox-exec` deprecated on macOS 15 | macOS 15 | Enforcement remains active; migration to Apple Container framework planned for v2 |

For full details, see [docs/security-model.md](docs/security-model.md).

---

## Coordinated disclosure timeline

After we receive a report:

1. Acknowledgment within 48 hours.
2. Severity assessment and target patch date within 14 days.
3. Fix developed and tested internally.
4. CVE requested through GitHub's advisory process if appropriate.
5. Fix and release notes published simultaneously with the CVE (if any).
6. Reporter credited in release notes unless they prefer anonymity.

We ask that reporters wait for the patch to be published before disclosing publicly.
If we miss the patch target date, we will communicate a revised timeline. If we are
more than 14 days past the original target for a critical issue, reporters are free
to disclose publicly at their discretion.

---

## Hall of fame

Researchers who responsibly disclose valid vulnerabilities will be acknowledged here
with their permission.

---

## Bug bounty

MCP Armor does not currently operate a formal bug bounty program. We offer public
credit in release notes and, for critical vulnerabilities, a personal thank-you from
the team. A bounty program may be introduced in a future release.
