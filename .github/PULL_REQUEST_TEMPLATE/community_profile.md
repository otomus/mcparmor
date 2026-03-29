## Community Profile Submission

### Tool Information

**Tool name:**
**Package name (npm/pip/go module/binary):**
**Tool version tested against:**
**Tested on (OS + version):**

---

### Capability Justification

For each capability declared in the profile, explain *why* the tool needs it.
"Just in case" is not a valid justification — every capability must be traceable
to documented tool behavior.

**filesystem.read:**
> Why does this tool need to read these paths?

**filesystem.write:**
> Why does this tool need to write to these paths?

**network.allow:**
> For each host:port entry, which documented feature of the tool uses it?

**spawn: true** *(requires two maintainer approvals)*:
> Why does this tool need to spawn child processes? What does it spawn?

**env.allow:**
> Which environment variables does the tool require and why?

---

### Minimality Confirmation

- [ ] I have tested the tool with this profile and confirmed it works correctly
- [ ] I have not declared any capability the tool does not actually use
- [ ] I have read the [minimality principle](../CONTRIBUTING.md#profile-minimality-principle)

---

### Validation

```bash
mcparmor validate --armor profiles/community/<tool-name>.armor.json
```

- [ ] `mcparmor validate` passes with no errors

---

### Additional Context

*(Optional: link to tool documentation, source code, or other evidence for the declared capabilities)*
