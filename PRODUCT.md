# MCP Armor — Product Plan

> **MCP tools, armored.**
> Capability protection for MCP tools. Every tool runs under a declared armor.

---

## The Problem

MCP (Model Context Protocol) is becoming the standard for AI tool integration. Every major AI product — Claude, Cursor, VSCode, ChatGPT, and every personal agent runtime — runs MCP tools as subprocesses. **None of them have a capability boundary model.**

An MCP tool runs with the same permissions as the host process. A community-contributed tool can read any file, call any host, spawn any process — and the runtime has no way to know or stop it. The ClawHavoc incident (January 2026) proved this is a real attack vector: hundreds of malicious skills in the OpenClaw registry harvested API keys from 93% of vulnerable instances.

**The gap:** MCP defines the communication protocol. It says nothing about what a tool is allowed to do on the host machine.

---

## The Solution

MCP Armor is a **manifest-driven capability enforcement layer** for MCP tool execution.

- Tool authors declare what their tool is authorized to do in a simple JSON manifest
- The community reviews whether those declarations are reasonable
- MCP Armor enforces them at runtime — cross-platform, no containers, no cloud required

**The tool ships with its own armor.**

---

## Core Concepts

### The Capability Manifest
Every tool declares its boundaries in `tool.json`:

```json
"armor": {
  "filesystem": ["read:/tmp/mcparmor/*"],
  "network": ["api.github.com"],
  "spawn": false,
  "env": ["GITHUB_TOKEN"]
}
```

This is the contract. MCP Armor enforces it.

### The Broker
A lightweight process that wraps every tool invocation:
- Intercepts filesystem access → checks against declared paths
- Intercepts network calls → checks against declared hosts
- Scans all output for leaked secrets before returning
- Blocks spawning of child processes unless declared
- Logs every action with full audit trail

### Trust Tiers
| Profile | Filesystem | Network | Spawn | Use case |
|---|---|---|---|---|
| `strict` | none | none | false | Fabricated / untrusted tools |
| `sandboxed` | tmp only | declared hosts | false | Community tools |
| `network` | none | declared hosts | false | API tools |
| `system` | declared paths | declared hosts | false | System tools |

---

## What Makes It Different

| | Bulwark | Orkia | E2B | OpenSandbox | **MCP Armor** |
|---|---|---|---|---|---|
| Model | Central proxy | Central runtime | Cloud execution | Cloud/container | Manifest-per-tool |
| Local-first | ✅ | ✅ | ❌ | ❌ | ✅ |
| No containers | ✅ | ✅ | ❌ | ❌ | ✅ |
| Tool ships its own policy | ❌ | ❌ | ❌ | ❌ | ✅ |
| Community-reviewed capabilities | ❌ | ❌ | ❌ | ❌ | ✅ |
| Protocol-aware (JSON-RPC) | ❌ | ❌ | ❌ | ❌ | ✅ |
| Cross-platform no prerequisites | ✅ | ❌ | ❌ | ❌ | ✅ |

**The key differentiator:** Bulwark, Orkia and others are proxy/gateway models — they enforce policy centrally at the runtime level. MCP Armor's policy lives in the tool manifest itself. The tool author declares capabilities. The community verifies them. Any runtime enforces them. No central proxy required.

---

## Target Audience

**Primary (v1):**
- MCP tool authors — declare capabilities, ship armored tools
- Agent runtime builders — integrate MCP Armor as their security layer
- Developers running personal agents (OpenClaw, NanoClaw, ZeroClaw, Nanobot, Arqitect)

**Secondary (v2+):**
- LLM framework builders — LangChain, CrewAI, AutoGen tool execution
- Any developer building a system that executes subprocess tools
- Enterprise teams auditing AI tool behavior

---

## Competitive Landscape

### Direct
- **Bulwark** (Anthropic, github.com/anthropics/bulwark) — MCP governance proxy, Rust, policy via YAML, audit trails. Central policy model.
- **Orkia** — Rust runtime, governance-first. Central model.
- **OneCLI** — Vault for AI agents in Rust.

### Indirect
- **E2B** — Cloud sandbox, Firecracker-powered. Cloud-dependent.
- **Alibaba OpenSandbox** — Released March 3, 2026. Docker/Kubernetes-based execution platform.
- **MCP Gateways** (Cerbos, various) — Network-level control, not execution-level.

### The Gap We Fill
Nobody has a **manifest-driven, tool-level, local-first** capability system where the policy travels with the tool. That's MCP Armor.

---

## Technical Architecture

### Components

```
mcparmor/
  spec/
    armor-manifest.schema.json     ← the capability declaration standard
  crates/
    mcparmor-core/                 ← Rust enforcement engine
    mcparmor-broker/               ← the broker process
    mcparmor-cli/                  ← CLI binary
  sdks/
    python/                        ← pip install mcparmor
    node/                          ← npm install mcparmor
  docs/
```

### How It Works

```
MCP Host → mcparmor run --manifest tool.json -- python tool.py
                ↓
         [Armor Broker]
         reads armor manifest
         wraps subprocess
         intercepts: fs / network / spawn / env
         scans: output for secrets
         logs: all actions
                ↓
         tool subprocess (JSON-RPC over stdio)
```

### Cross-Platform Strategy
- **macOS:** OS-native Seatbelt profiles generated from manifest (belt-and-suspenders)
- **Linux:** Seccomp + Landlock profiles generated from manifest
- **Windows:** AppContainer rules generated from manifest
- **Everywhere:** Protocol-layer broker enforcement regardless of OS

OS primitives are an upgrade layer — the broker works on all platforms without them.

---

## Scope Progression

```
v1:  MCP tool execution protection
     → manifest spec, broker, CLI, Python + Node SDKs
     → works with any MCP host

v2:  Any subprocess tool (JSON-RPC over stdio)
     → expand beyond MCP protocol
     → Arqitect mcp_tools integration

v3:  Universal capability standard
     → OS-native profile generation
     → community-verified capability registry
```

---

## Go-to-Market

### Launch Strategy
1. **Ship Arqitect with MCP Armor** — reference implementation, proves it works in production
2. **Announce separately** — MCP Armor as its own project, Arqitect as the primary consumer
3. **Target MCP ecosystem directly** — MCP tool authors, Cursor/Claude/VSCode communities
4. **Write the "ClawHavoc post-mortem" article** — explain the attack, explain the fix, introduce MCP Armor

### Distribution
- GitHub (primary discovery)
- Hacker News launch — the Bulwark, Orkia, Forge precedent shows this audience is receptive
- MCP community channels
- Dev.to / Medium technical writeup

### Positioning
Not "another governance proxy." The message:

> *Other tools protect the runtime. MCP Armor protects the tool. Every tool ships with its own declared capabilities — reviewed by the community, enforced at execution.*

---

## Open Source Model

- **License:** MIT
- **Governance:** Arqitect team maintains core spec and broker
- **Community:** contributes armor profiles for popular MCP tools
- **Monetization (future):** hosted capability registry with verified profiles, enterprise audit dashboard

---

## Milestones

| Milestone | Description |
|---|---|
| **M0 — Spec** | Define armor manifest JSON schema. Publish to GitHub. |
| **M1 — Broker MVP** | Rust broker: filesystem + network interception, secret scanning, audit log |
| **M2 — CLI** | `mcparmor run` command. Works on macOS + Linux. |
| **M3 — Python SDK** | `pip install mcparmor`. Arqitect integration. |
| **M4 — Node SDK** | `npm install mcparmor`. OpenClaw/NanoClaw integration path. |
| **M5 — Launch** | HN post. Reference implementation live. Arqitect ships with it. |
| **M6 — OS Primitives** | Seccomp + Landlock (Linux), Seatbelt (macOS) profile generation |
| **M7 — Community Registry** | Verified armor profiles for popular MCP tools |

---

## Relationship to Arqitect

MCP Armor is built by the Arqitect team to solve Arqitect's own security problem. Arqitect is the reference consumer — it ships with MCP Armor as its tool execution layer.

The projects are separate:
- MCP Armor = neutral infrastructure, MIT licensed, community owned
- Arqitect = the agent platform that uses it

This separation is intentional. It signals to other frameworks that MCP Armor is ecosystem infrastructure, not Arqitect-proprietary tooling. Other runtimes adopting it is a bonus — not the primary goal.
