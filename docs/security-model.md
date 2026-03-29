# MCP Armor Security Model

This document describes what MCP Armor enforces, how it enforces it, where it does not enforce, and why. It is written for users who need to make an informed risk decision before deploying MCP Armor in a production environment.

---

## The Two-Layer Model

MCP Armor enforces capability isolation through two independent layers. Both layers read the same armor manifest. A failure or gap in one layer does not defeat the other.

### Layer 1 — Protocol Proxy (all platforms, always active)

The broker sits between the MCP host and the tool subprocess, forwarding JSON-RPC messages over stdio. Before forwarding any message, it inspects the content against the declared manifest.

| What Layer 1 enforces | Mechanism |
|---|---|
| Environment variable restriction | Strips undeclared variables from the subprocess environment at spawn time |
| Filesystem path validation in params | Inspects JSON-RPC parameters before forwarding; blocks calls containing undeclared paths |
| URL validation in params | Same — blocks calls containing undeclared hostnames |
| Secret and credential scanning in responses | Runs pattern matching on every response; redacts matches before the host sees them |
| Timeout | Sends SIGTERM then SIGKILL if the tool exceeds its declared timeout |
| Output size cap | Truncates responses that exceed `max_bytes` |

Layer 1 is reliable for protocol-path attacks — cases where the host sends a request and the tool responds via JSON-RPC. It does not see direct syscalls made by the tool binary.

### Layer 2 — OS Sandbox (Linux + macOS, active by default)

The broker configures a kernel-level sandbox before spawning the tool. The sandbox enforces the same constraints as Layer 1, but at the syscall level — independent of the tool's language, runtime, or the content of any JSON-RPC message.

| What Layer 2 enforces | Mechanism |
|---|---|
| Filesystem read restrictions | Landlock (Linux 5.13+) or Seatbelt (macOS) |
| Filesystem write restrictions | Landlock (Linux 5.13+) or Seatbelt (macOS) |
| Outbound network filtering | Seatbelt hostname-level (macOS), Landlock TCP port-level (Linux 6.7+) |
| Child process spawning | Seccomp `exec` filter (Linux 3.5+), Seatbelt `process-exec` deny (macOS) |

Layer 2 works regardless of tool language. A Go binary, a Python script, and a Rust binary all make the same syscalls to the same kernel. The sandbox intercepts at that level.

---

## Per-Platform Enforcement Table

| Platform | Layer 1 | Layer 2 | Notes |
|---|---|---|---|
| macOS 12+ | Full | Seatbelt | Full enforcement including hostname-level network blocking. |
| Linux 6.7+ | Full | Seccomp + Landlock (FS + TCP ports) | Full filesystem and TCP port enforcement. Hostname filtering is Layer 1 only. |
| Linux 5.13–6.6 | Full | Seccomp + Landlock (FS only) | Filesystem enforced at kernel level. No kernel-level TCP port filtering. Network enforced by Layer 1 param inspection only. |
| Linux 3.5–5.12 | Full | Seccomp (spawn only) | Spawn blocked. No kernel-level filesystem or network isolation. |
| Windows | Full | None (v1) | Protocol-layer only. Kernel enforcement planned for v3 via AppContainer. |

`--no-os-sandbox` disables Layer 2 as an escape hatch for environments where it fails. The default is maximum enforcement. Opt-out, not opt-in.

---

## Adversarial Test Matrix

The following tests use compiled binaries (Go, Python, JavaScript) that attempt to violate an armor manifest at the syscall level — bypassing Layer 1 entirely by not using JSON-RPC. This is the realistic adversary model.

| Test | Tool | macOS | Linux 5.13–6.6 | Linux 6.7+ | Linux < 5.13 | Windows |
|---|---|---|---|---|---|---|
| `read_passwd` | .go / .py / .js | BLOCKED (Seatbelt) | BLOCKED (Landlock FS) | BLOCKED (Landlock FS) | ALLOWED ⚠ | ALLOWED ⚠ |
| `call_forbidden` (port 80) | .go | BLOCKED (Seatbelt hostname) | ALLOWED ⚠ | BLOCKED (Landlock TCP) | ALLOWED ⚠ | ALLOWED ⚠ |
| `call_forbidden` (port 443) | .go | BLOCKED (Seatbelt hostname) | ALLOWED ⚠ | ALLOWED ⚠ | ALLOWED ⚠ | ALLOWED ⚠ |
| `call_metadata` (169.254.169.254) | .go | BLOCKED (Seatbelt) | ALLOWED ⚠ | ALLOWED ⚠ | ALLOWED ⚠ | ALLOWED ⚠ |
| `call_metadata` (169.254.169.254) | .py / .js | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) |
| `leak_secret` | all | BLOCKED (scanner) | BLOCKED (scanner) | BLOCKED (scanner) | BLOCKED (scanner) | BLOCKED (scanner) |
| `spawn_child` | .go / .py / .js | BLOCKED (Seatbelt) | BLOCKED (Seccomp) | BLOCKED (Seccomp) | BLOCKED (Seccomp) | ALLOWED ⚠ |
| `path_traversal` | any | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) | BLOCKED (Layer 1) |

### Explanation of ALLOWED cells

**`read_passwd` on Linux < 5.13:**
Landlock filesystem isolation requires Linux 5.13. On older kernels, only spawn blocking (Seccomp) is available. A compiled binary can open `/etc/passwd` directly. Mitigation: upgrade to Linux 5.13+ or run in a container with an overlayfs that excludes sensitive paths.

**`call_forbidden` (port 80) on Linux 5.13–6.6:**
Landlock TCP port filtering requires Linux 6.7. On earlier kernels, network enforcement is Layer 1 only (param inspection). A compiled binary making a direct `connect()` syscall to port 80 is not blocked at the kernel level. Mitigation: upgrade to Linux 6.7+, or use network namespace isolation at the container/VM level.

**`call_forbidden` (port 443) on Linux 6.7+:**
Landlock TCP blocks by port number, not by hostname. If the armor manifest declares `api.example.com:443`, Landlock allows all outbound traffic to port 443 — including to undeclared hosts. This is the documented Linux hostname gap (see below). Mitigation: use macOS for hostname-level enforcement, or add network namespace rules at the infrastructure level.

**`call_forbidden` (port 80) on Linux < 5.13, Windows:**
No kernel-level network enforcement on these platforms. See entries above and below for context.

**`call_metadata` on Linux (all versions):**
The AWS/GCP metadata endpoint at `169.254.169.254` is reachable via direct `connect()` from compiled binaries on all Linux versions, because hostname-based filtering is Layer 1 only on Linux. Python and JavaScript tools are blocked by Layer 1 because they use the MCP JSON-RPC path. Compiled Go binaries bypass Layer 1. Mitigation: block the metadata range at the network infrastructure level (VPC/subnet rules), or use macOS.

**`call_metadata` on Windows:**
No kernel-level enforcement in v1. Layer 1 blocks Python and JavaScript tools via param inspection. Compiled binaries are not blocked. Mitigation: use network-level controls or run on Linux/macOS.

**`spawn_child` on Windows:**
No kernel-level process execution controls in v1. Spawn blocking is implemented on Linux via Seccomp and on macOS via Seatbelt. Windows kernel enforcement is planned for v3. Mitigation: use OS-level job objects or run on Linux/macOS.

---

## The Linux Hostname Gap

Linux's Landlock security module, as of v6.7, supports TCP port filtering. It does not support hostname filtering. This is not a bug — it reflects how the Linux kernel network stack works. Hostname resolution happens in userspace (glibc, the resolver). By the time a `connect()` syscall reaches the kernel, it operates on an IP address, not a hostname.

What this means in practice:

- A manifest declaring `network.allow: ["api.github.com:443"]` on Linux 6.7+ will enforce that the tool can only connect on port 443.
- It will not enforce that the tool can only connect to `api.github.com`. Any host on port 443 is reachable.

macOS Seatbelt resolves hostnames at rule-application time and can express true hostname-level constraints. This is why macOS provides stronger network enforcement than Linux for tools that need tight outbound allow-lists.

The platform enforcement table above documents this gap explicitly. MCP Armor does not hide it.

---

## What MCP Armor Does Not Protect Against

Being explicit about scope is a requirement for a credible security tool. The following threats are out of scope for MCP Armor v1.

| Threat | Why out of scope | What does mitigate it |
|---|---|---|
| **Prompt injection** | A tool returning malicious text designed to hijack the AI's next action | Host-level input/output filtering; a different layer entirely |
| **Malicious `armor.json`** | A tool author can declare less than they actually need | Community profile review; source code audit |
| **HTTP/remote MCP tools** | No subprocess to wrap; the tool runs in remote infrastructure | The remote provider's own security controls |
| **The AI model itself** | Jailbreaking, hallucination, model-level attacks | Model provider controls; not a tool execution problem |
| **Kernel CVEs** | If the OS sandbox primitive has a vulnerability, MCP Armor inherits it | Patch your OS |
| **Tool author identity** | MCP Armor does not verify who wrote or signed a tool | Package registry signing (npm provenance, pip sigstore) |

---

## Responsible Disclosure

To report a vulnerability in MCP Armor's enforcement — a broker bypass, OS sandbox escape, or scanner evasion — see [SECURITY.md](../SECURITY.md).

Do not open a public GitHub issue for security vulnerabilities.
