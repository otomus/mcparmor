# Contributing to MCP Armor

There are two ways to contribute: **community armor profiles** (the most common path) and **code contributions** to the broker, CLI, or SDK.

---

## Community Profile Submissions

A community profile is an `armor.json` file that declares the exact capabilities a specific MCP tool needs to run. Profiles are the primary surface the community contributes to, and the quality of the profile library is directly proportional to how useful MCP Armor is to its users.

### What a profile is

A profile is a machine-readable manifest that tells the MCP Armor broker exactly what a tool is allowed to do: which filesystem paths it may read or write, which network hosts it may reach, whether it may spawn child processes, and which environment variables it requires.

The broker enforces these declarations at the OS level. A profile that is too permissive erodes trust; a profile that is too restrictive breaks the tool. Getting it right matters.

### Where profiles live

```
profiles/community/<tool-name>.armor.json
```

Use the tool's canonical name in lowercase, hyphenated. Examples:
- `profiles/community/github.armor.json`
- `profiles/community/filesystem-server.armor.json`
- `profiles/community/postgres.armor.json`

### PR template fields (required)

Every community profile PR must fill out all of the following in the PR description:

| Field | Description |
|---|---|
| **Tool name** | Human-readable name of the MCP tool |
| **Package name** | The npm package, pip package, Go module, or binary name |
| **Why each capability is needed** | Per-capability justification (see below) |
| **Tested on** | OS name and version (e.g. macOS 14.4, Ubuntu 22.04) |
| **Tool version tested against** | Exact version string |

For the capability justification, every declared capability must be explained individually:

- `filesystem.read` — which paths and why the tool reads them
- `filesystem.write` — which paths and why the tool writes to them
- `network.allow` — for each `host:port`, which documented feature of the tool uses it
- `spawn: true` — what the tool spawns and why (requires two maintainer approvals)
- `env.allow` — which environment variables are required and what they configure

### Review criteria

Maintainers evaluate every profile submission against the following checklist. A PR can be closed for failing any single criterion.

**Is the profile minimal?**
Every declared capability must be traceable to documented tool behavior. "Just in case," "might need it," and "safer to include" are not valid justifications. When in doubt, leave it out — the tool will fail loudly if it needs something undeclared.

**Is the network allowlist tight?**
Wildcard subdomains (`*.example.com`) are acceptable when the tool reaches multiple subdomains of a known provider. A catch-all (`*:*`) is not acceptable under any circumstances.

**Is `spawn: true` justified?**
Spawning child processes is a high-risk capability. It requires an explicit, documented reason and two maintainer approvals. "The tool is a shell wrapper" is acceptable; "I'm not sure if it spawns" is not.

**Does the profile match what the tool actually does?**
Maintainers will cross-reference the declared capabilities against the tool's published documentation and, for open-source tools, its source code. Profiles that misrepresent tool behavior — either by over-declaring or under-declaring — will be returned for revision.

### Auto-close policy

The following conditions trigger an immediate close without review:

- **Regression**: any PR where a `qualification_score` in any tier is lower than the current value on `main`
- **Duplicate**: any PR for a tool that already has an open PR — the newer PR is closed

A brief comment explaining the reason is posted before closing.

### Review SLA

| Condition | SLA |
|---|---|
| Schema-valid PR | Reviewed within 5 business days |
| PR with `spawn: true` or `filesystem.read: ["/"]` | Requires two maintainer approvals |
| Tool with > 10k weekly downloads | Expedited review |

### Trust progression

| Milestone | Effect |
|---|---|
| First 3 profiles | Full maintainer review required |
| 3 profiles merged | Trusted contributor — single maintainer approval |
| 10 profiles merged | Can self-merge simple updates if CI passes and no `spawn` changes |

---

## Profile Schema Validation

Before opening a PR, validate your profile locally:

```bash
mcparmor validate --armor profiles/community/<tool-name>.armor.json
```

The command exits non-zero on any schema error and prints the specific field that failed. Fix all errors before submitting — PRs that fail schema validation are not reviewed until they pass.

---

## CI Gates

Every profile PR runs the following automated checks:

| Check | Blocking? |
|---|---|
| Schema validation against `spec/v1.0/armor.schema.json` | Yes — PR cannot merge |
| Warning if `network.allow` contains `*:*` | No — warning only |
| Warning if `filesystem.read` contains `/` (root) | No — warning only |
| Warning if `spawn: true` without PR justification | No — warning only |
| Capability diff comment posted to the PR (for updates) | Informational |

The capability diff comment shows exactly what changed between the existing profile and the proposed one, expressed in plain English. This makes it easy for maintainers to see the delta without reading raw JSON.

---

## Code Contributions

Code contributions touch the broker (Rust), CLI (Rust), or SDK. They are held to a higher bar than profile submissions because bugs in the broker directly affect enforcement.

### Before submitting

```bash
cargo test
cargo clippy -- -D warnings
```

Both must pass with no errors or warnings. A PR that fails either will not be reviewed.

### Requirements

- All exported functions must have doc comments explaining the contract, not just restating the name.
- No `unwrap()` in production code paths. Use `?` with `anyhow::Context` to propagate errors with context.
- Any new enforcement logic requires tests. Tests must include adversarial cases — not just the happy path.
- Sandbox changes (Seatbelt profiles, Landlock rules, Seccomp filters) require adversarial tests using compiled Go or C binaries that attempt to violate the new rule. A test that only verifies the broker doesn't crash is insufficient.

---

## Profile Minimality Principle

**Declare only what the tool actually needs.**

This is the single most important rule in this document. A profile that requests more than necessary is not a safe default — it is a liability. Users trust community profiles to reflect the true capability surface of a tool. An over-declared profile that grants network access the tool never uses provides false assurance and widens the blast radius of any compromise.

If you are unsure whether a tool needs a given capability, do not declare it. Run the tool without it. If it fails, the error message will tell you exactly what it needed. That is the evidence you need to justify the capability in your PR.

A profile that is rejected for over-declaration is not a failure — it is the review process working correctly.
