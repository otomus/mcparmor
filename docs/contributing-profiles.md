# Contributing Community Profiles

This document describes how to contribute an `armor.json` profile for a popular MCP
tool to the `profiles/community/` directory. Community profiles let any user run a
tool under MCP Armor without writing a manifest themselves — they pull a reviewed
profile and get enforcement immediately.

A community profile is a trust artifact. It carries the reviewer's name and the date
of review. Getting it wrong means users run tools with looser restrictions than they
believe they have. The review process exists to prevent that.

---

## What makes a good community profile

A good profile has exactly the capabilities the tool needs and nothing more. The
minimality principle is the single most important criterion: every declared capability
must be traceable to a documented behavior of the tool.

**Good:**
- `network.allow: ["api.github.com:443"]` — The GitHub MCP server documents that it
  calls `api.github.com`. One host, one port, justified.
- `env.allow: ["GITHUB_TOKEN"]` — The tool explicitly requires this token to
  authenticate. One variable, justified.

**Not good:**
- `network.allow: ["*:443"]` — Wildcard host when the tool only needs one specific
  host. Unjustified and rejected.
- `filesystem.read: ["/home/*"]` — Broad home directory access when the tool only
  reads from a specific subdirectory.
- `spawn: true` — Process spawning enabled without documented justification.

The question the reviewer asks for every field: "Does the tool's published
documentation or source code explain why this capability is needed?"

---

## Before you start

1. Read `docs/manifest-spec.md` — you need to understand every field before writing
   a profile.
2. Read `docs/security-model.md` — you need to understand the enforcement model to
   know whether your profile provides meaningful restrictions.
3. Check whether the tool already has a profile in `profiles/community/`. If it does,
   open a PR to update the existing one rather than creating a new file.
4. Check whether another PR is open for the same tool. Duplicate PRs are closed
   without review — the open PR takes precedence.

---

## Profile file naming

Profile files follow the naming convention `<tool-slug>.armor.json`, where
`<tool-slug>` is the tool's npm package name (without scope), PyPI package name, or
a descriptive slug for non-packaged tools. Use lowercase and hyphens.

Examples:
- `github.armor.json` for `@modelcontextprotocol/server-github`
- `playwright.armor.json` for the Playwright MCP server
- `brave-search.armor.json` for the Brave Search MCP server

---

## Quality requirements

Every submitted profile must pass all of the following before it is merged.

### Schema validation

The profile must pass JSON Schema validation against `spec/v1.0/armor.schema.json`.

```bash
mcparmor validate profiles/community/your-tool.armor.json
```

Validation must exit `0` with no errors. Warnings must be addressed or explicitly
justified in the PR description.

### Required fields

Every community profile must include:

| Field | Requirement |
|---|---|
| `$schema` | Must be the canonical v1.0 URI |
| `version` | Must be `"1.0"` |
| `min_spec` | Must be `"1.0"` |
| `profile` | Required |
| `_source` | Must be `"community"` |
| `_reviewed_by` | Filled by the reviewer during merge — leave blank in submission |
| `_reviewed_at` | Filled by the reviewer during merge — leave blank in submission |

### Minimality

Every declared capability must be justified in the PR description. The PR template
(below) asks you to fill in one line per capability explaining why it is needed.

Profiles that declare capabilities not supported by documented tool behavior are
rejected with a comment explaining which capabilities are unsupported.

### No wildcards without justification

- `network.allow: ["*:443"]` — requires a written explanation of why the tool needs
  to connect to arbitrary HTTPS hosts. Accepted only for tools where this is inherent
  to the tool's purpose (e.g. an SSL certificate checker).
- `network.allow: ["*.example.com:443"]` — requires the same justification.
- `spawn: true` — requires the tool's documentation to be cited showing that process
  spawning is an explicit feature.

### `deny_metadata: true` is non-negotiable

Community profiles must not set `deny_metadata: false`. If a submitted profile
includes this, it will be rejected without exception. There is no legitimate MCP tool
that needs access to cloud instance metadata endpoints.

### `output.scan_secrets`

Community profiles should enable secret scanning. The default recommendation is
`scan_secrets: true` (redact). Use `scan_secrets: "strict"` for tools that handle
authentication tokens or credentials in their responses (e.g. Slack, GitHub with
fine-grained tokens). Disabling secret scanning (`scan_secrets: false`) requires
justification in the PR description.

### Testing

Before submitting, test the profile with a real invocation of the tool:

```bash
mcparmor run --armor profiles/community/your-tool.armor.json -- <tool command>
```

Verify that:
1. The tool starts successfully.
2. Common tool operations work without capability violations.
3. The tool cannot access filesystem paths or network hosts beyond what is declared.

If you cannot test locally (e.g. the tool requires credentials you don't have), note
this explicitly in the PR description.

---

## PR template guidance

When you open a PR adding a community profile, include the following information in
the PR description. Copy this template and fill it in.

```markdown
## Tool

<!-- Name and brief description of the MCP tool this profile covers. -->
<!-- Link to the tool's repository or documentation. -->

## Profile

<!-- Paste the full contents of your armor.json here. -->

## Capability justification

<!-- One line per declared capability explaining why it is needed. -->
<!-- Cite the tool's documentation or source code where possible. -->

**network.allow:**
- `api.github.com:443` — The tool calls the GitHub REST API at this endpoint.
  Source: https://github.com/modelcontextprotocol/servers/blob/main/src/github/...

**env.allow:**
- `GITHUB_TOKEN` — Required for GitHub API authentication.
  Source: Tool README, "Configuration" section.

**filesystem.read:** (if applicable)
- Why the tool needs these paths.

**spawn: true:** (if applicable)
- The tool must launch a subprocess because: ...

**scan_secrets choice:**
- Why you chose `true` vs `"strict"` vs `false`.

## How I tested this

<!-- Describe what you ran and what you verified. -->
<!-- Include the mcparmor CLI version and platform (macOS 14, Linux 6.7, etc.) -->

## Known gaps or caveats

<!-- Anything the reviewer should know that isn't captured in the profile itself. -->
```

---

## Trust progression

Profiles in `profiles/community/` carry a `_source` and `_reviewed_by` field that
record their provenance and review status. Three levels exist:

### `"team-authored"`

Written by the MCP Armor core team as launch-time profiles. These were created before
the community PR process opened, held to the same minimality standard, and reviewed
internally. They are not submitted through the community PR process but carry the same
enforced minimality requirement.

All profiles present in the repository at the v1.0 release date are team-authored.

### `"community"` — submitted, reviewed by a named maintainer

Submitted by a community contributor and reviewed by at least one named maintainer
listed in `CODEOWNERS`. The reviewer's GitHub username appears in `_reviewed_by`.

A community-reviewed profile has been checked for minimality, schema conformance, and
wild-card justification. It has not necessarily been tested against every version of
the tool or in every environment.

### Upgrading a community profile to team-authored

If a community profile has been in use for at least 3 months without reported issues,
the core team may re-review it, update `_source` to `"team-authored"`, and take
responsibility for future updates. This is informal — the key signal is track record
and the team's confidence in the tool's behavior.

---

## Review criteria checklist

Reviewers use this checklist when evaluating a PR.

- [ ] Profile file is named correctly (`<slug>.armor.json`)
- [ ] `$schema` is the canonical v1.0 URI
- [ ] `version` is `"1.0"`, `min_spec` is `"1.0"`
- [ ] `profile` is appropriate for the tool type
- [ ] `_source` is `"community"`
- [ ] `deny_metadata: true` (no exception)
- [ ] No `*:*` wildcard in `network.allow`
- [ ] Every `network.allow` entry is justified in the PR description
- [ ] Every `env.allow` entry is justified
- [ ] `spawn: true` is justified and traced to documented tool behavior
- [ ] `scan_secrets` is enabled unless justified otherwise
- [ ] `mcparmor validate` passes with no errors
- [ ] PR description includes a testing section
- [ ] No open PR already exists for this tool

If any item fails, the reviewer leaves a comment explaining what is needed and
requests changes. The PR is not merged until all items pass.

---

## How profiles are updated

Profiles must be updated when a tool's behavior changes in ways that affect the
required capabilities — for example, a new version of the GitHub MCP server starts
calling a new API endpoint that wasn't in the `network.allow` list.

**To update an existing profile:**

1. Open a PR modifying the existing `profiles/community/<tool>.armor.json`.
2. Describe what changed in the tool's behavior and why the profile needs updating.
3. Do not change `_reviewed_by` or `_reviewed_at` — these are updated by the reviewer
   on merge.

**Profile integrity verification:**

The MCP Armor broker verifies profiles pulled from the community registry using
SHA-256 checksums stored in a signed manifest. When `mcparmor profiles update` fetches
a new or updated profile:

1. The broker downloads the profile file.
2. It verifies the SHA-256 checksum against the signed manifest.
3. If the checksum does not match, the update is rejected and the existing profile is
   kept.
4. The signed manifest is verified using a public key bundled with the broker.

This prevents a compromised distribution channel from delivering a modified profile
that grants more capabilities than reviewed. The profile you use is cryptographically
verified to be the one a maintainer reviewed and signed.

**Local profile overrides are not verified.** Profiles loaded directly from the
filesystem via `--armor /path/to/armor.json` are trusted as-is. The verification
mechanism applies only to profiles distributed through the registry.

---

## After your PR is merged

Once merged, your profile will be included in the next `mcparmor profiles update`
and bundled in the next broker release. Users can install it immediately:

```bash
mcparmor profiles update
mcparmor profiles show <tool-slug>
mcparmor run --armor $(mcparmor profiles path <tool-slug>) -- <tool command>
```

Your GitHub username will appear in `_reviewed_by` (if you are the reviewer) or in
the git commit authorship. The `_reviewed_at` date will reflect the merge date.

---

## Questions and profile requests

If you want a profile for a specific tool but cannot write or test it yourself, open
a GitHub issue with the label `profile-request`. The issue should include the tool
name, a link to its source, and any specific capabilities you know it needs. The
community or core team may pick this up.

Do not open a PR with a profile you cannot validate — partially-tested profiles are
not accepted. If you lack the credentials to test (e.g. you don't have a Slack
workspace), note this in a `profile-request` issue rather than submitting an untested
PR.
