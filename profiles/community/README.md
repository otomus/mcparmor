# Community Profiles

Community profiles are `armor.json` files that declare the exact capabilities a
specific MCP tool needs to run safely under MCP Armor. Each profile is the result of
a review process that verifies the declared capabilities match what the tool actually
does — no more.

When you use a community profile, you are relying on a reviewer's judgment that the
declared capabilities are minimal and accurate. Profiles that declare more than the
tool needs are rejected. See `docs/contributing-profiles.md` for the full review
criteria.

---

## How profiles work

When you run a tool through MCP Armor, the broker reads the tool's armor profile and
configures two enforcement layers:

1. **Layer 1 (protocol proxy)** — the broker intercepts all MCP JSON-RPC traffic and
   blocks any request that falls outside the declared manifest, regardless of OS or
   platform.
2. **Layer 2 (OS sandbox)** — on supported platforms, the broker applies a
   kernel-level sandbox (Seatbelt on macOS, Landlock + Seccomp on Linux) that
   enforces the same constraints at the syscall level.

The profile is the source of truth for both layers. For the full enforcement model,
see `docs/security-model.md`.

---

## Profile provenance levels

Profiles carry a `_source` field that records how they entered the library.

| Value | Meaning |
|---|---|
| `"team-authored"` | Written by the MCP Armor core team. Reviewed internally, held to the same minimality standard, and not subject to the community PR process. All profiles present at the v1.0 release are team-authored. |
| `"community"` | Submitted by a community contributor and reviewed by at least one named maintainer. The reviewer's GitHub username appears in `_reviewed_by`. |

Post-launch profiles go through the full community review process documented in
`docs/contributing-profiles.md`.

---

## Included profiles

| Profile file | Tool | Profile type | Notable capabilities |
|---|---|---|---|
| `github.armor.json` | `@modelcontextprotocol/server-github` | `network` | `api.github.com:443`, `github.com:443`, GITHUB_TOKEN, secret scanning |
| `filesystem.armor.json` | `@modelcontextprotocol/server-filesystem` | `sandboxed` | `/tmp/mcparmor/*` read/write only |
| `playwright.armor.json` | Playwright MCP server | `browser` | `*:443`, `*:80`, `spawn: true`, CDP access (`deny_local: false`) |
| `git.armor.json` | `@modelcontextprotocol/server-git` | `sandboxed` | `/tmp/mcparmor/*` r/w, no network |
| `sqlite.armor.json` | SQLite MCP server | `sandboxed` | `/tmp/mcparmor/*.db`, `/tmp/mcparmor/*.sqlite` r/w |
| `slack.armor.json` | Slack MCP server | `network` | `slack.com:443`, `api.slack.com:443`, `files.slack.com:443`, strict secret scanning |
| `notion.armor.json` | Notion MCP server | `network` | `api.notion.com:443`, NOTION_API_KEY |
| `brave-search.armor.json` | Brave Search MCP server | `network` | `api.search.brave.com:443`, BRAVE_API_KEY |
| `fetch.armor.json` | Generic HTTP fetch tool | `network` | `*:443`, `*:80` — wildcard justified by tool purpose |
| `gmail.armor.json` | Gmail MCP server | `network` | Google OAuth endpoints, strict secret scanning, `redact_params: true` |

All included profiles set `deny_metadata: true`. None set `deny_metadata: false`.
All include `spawn: false` except `playwright.armor.json`, where spawning is required
to launch the browser subprocess.

---

## Discovery and updates

Profiles are bundled into the MCP Armor binary at each release. To fetch profiles
added or updated since your installed version:

```bash
mcparmor profiles update
```

The update process verifies the SHA-256 checksum of each downloaded profile against
a signed manifest. A profile whose checksum does not match is rejected — the existing
version is kept and an error is logged. This prevents a compromised distribution
channel from delivering a modified profile.

To list all available profiles:

```bash
mcparmor profiles list
```

To inspect a specific profile before using it:

```bash
mcparmor profiles show github
```

This prints the full profile in a human-readable format, including every declared
capability, the schema version, the reviewer, and the review date.

To show the filesystem path to a profile (useful for `--armor` flags):

```bash
mcparmor profiles path github
```

---

## How to use a profile

### Inline, with `mcparmor run`

```bash
mcparmor run --armor profiles/community/github.armor.json -- \
  npx @modelcontextprotocol/server-github
```

Using the profile registry path:

```bash
mcparmor run --armor $(mcparmor profiles path github) -- \
  npx @modelcontextprotocol/server-github
```

### Auto-discovery, with `mcparmor wrap`

If a profile exists in the community library for the tool you are wrapping,
`mcparmor wrap` will find and apply it automatically:

```bash
mcparmor wrap --host claude-desktop
```

`mcparmor wrap` matches tools by command name (e.g. `npx @modelcontextprotocol/server-github`)
to community profiles. When a match is found, the discovered profile path is written
into the wrapped config. When no match is found, the tool runs under the `strict`
fallback profile with a warning.

You can always override auto-discovery with an explicit `--armor` flag.

### In an MCP host config (e.g. Claude Desktop)

```json
{
  "mcpServers": {
    "github": {
      "command": "mcparmor",
      "args": [
        "run",
        "--armor", "/home/user/.mcparmor/profiles/community/github.armor.json",
        "--",
        "npx", "-y", "@modelcontextprotocol/server-github"
      ],
      "env": {
        "GITHUB_TOKEN": "ghp_..."
      }
    }
  }
}
```

Use absolute paths. The `~` shorthand is not expanded when MCP hosts spawn
subprocesses directly.

---

## Trust model

Profiles are reviewed for minimality by named maintainers listed in `CODEOWNERS`.
The review process verifies that every declared capability is traceable to documented
tool behavior. Profiles that request more than the tool needs are rejected.

A profile cannot grant more than it declares: even if a profile lists a network host,
the OS sandbox enforces that constraint independently at the kernel level (where
available). The profile and the sandbox are redundant by design — a mistake in one
does not defeat the other.

Users can audit any profile at any time:

```bash
mcparmor profiles show <name>
```

No profile is applied silently. MCP Armor always tells you which profile is active
and where it came from before running any tool.

---

## How to submit a new profile

1. Read `docs/contributing-profiles.md` for the full process, quality requirements,
   and PR template.
2. Verify no existing profile covers the tool (`mcparmor profiles list`).
3. Verify no open PR already exists for the tool.
4. Write the profile following the minimality principle: every capability must be
   traceable to the tool's documented behavior.
5. Validate: `mcparmor validate profiles/community/your-tool.armor.json`
6. Test with a real invocation of the tool.
7. Open a PR with the PR template filled in (including capability justifications and
   a testing section).

Community profiles must set `_source: "community"`. Leave `_reviewed_by` and
`_reviewed_at` blank — these are filled by the reviewer at merge time.

Profile requests (for tools you cannot write or test yourself) can be opened as
GitHub issues with the `profile-request` label.
