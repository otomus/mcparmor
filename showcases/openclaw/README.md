# MCP Armor — OpenClaw

> **Validation status: PENDING** — This showcase has not been verified against
> the production OpenClaw runtime. The integration approach is correct;
> end-to-end validation is in progress.

OpenClaw is a skill-based MCP host that launches individual skills as
subprocesses. Unlike config-file hosts, OpenClaw uses a programmatic skill
runner — so integration uses the Node.js SDK (`armorSpawn`) rather than
`mcparmor wrap`.

---

## Integration approach

OpenClaw's skill runner calls `child_process.spawn` to launch each skill. The
integration replaces that call with `armorSpawn` from the `mcparmor` npm
package. This is a one-line change per spawn site.

**Before:**
```js
const { spawn } = require('child_process');
const proc = spawn(skill.command, skill.args, { stdio: 'pipe' });
```

**After:**
```js
const { armorSpawn } = require('mcparmor');
const proc = armorSpawn(skill.command, skill.args, {
  armor: skill.armorPath,
  stdio: 'pipe',
});
```

Each skill carries its own `armor.json` manifest that declares exactly what it
needs. Everything not declared is denied at the broker layer.

---

## File overview

| File | Purpose |
|---|---|
| `skill-runner.js` | Full `SkillRunner` class showing before/after comments at every spawn site |
| `example-skill/tool.js` | A sample skill (weather lookup) used to exercise the runner |
| `example-skill/armor.json` | The manifest for the example skill — declares network access to weather APIs only |

---

## Setup

1. Install the mcparmor Node.js SDK:
   ```bash
   npm install mcparmor
   ```

2. Add `armorPath` to each skill descriptor in your OpenClaw config, pointing
   to the skill's `armor.json`.

3. Replace `spawn` with `armorSpawn` in your skill runner (see `skill-runner.js`
   for a complete example).

4. Restart the OpenClaw runtime.

---

## Example skill manifest

The `example-skill/armor.json` shows a realistic manifest for a skill that
calls external weather APIs:

```json
{
  "profile": "network",
  "network": {
    "allow": ["api.openweathermap.org:443", "api.open-meteo.com:443"],
    "deny_local": true,
    "deny_metadata": true
  },
  "spawn": false,
  "env": { "allow": ["OPENWEATHER_API_KEY"] },
  "output": { "scan_secrets": true }
}
```

The skill can reach the declared weather endpoints. All other network
destinations, local addresses, and cloud metadata endpoints are denied.

---

## Reverting

Remove `mcparmor` from `package.json` and revert the `armorSpawn` call back to
`spawn`. No config files are modified by this integration.
