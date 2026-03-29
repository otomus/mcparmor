/** Result of an adversarial test cell: blocked or allowed with a reason. */
export interface MatrixCell {
  /** Whether the attack was blocked or allowed. */
  status: "blocked" | "allowed" | "informational";
  /** The enforcement layer that blocked it (if blocked). */
  layer?: string;
}

/** A single row in the adversarial test matrix. */
export interface AdversarialTest {
  /** Short identifier for the test. */
  id: string;
  /** Human-readable name. */
  name: string;
  /** What the compiled Go binary does in this test. */
  description: string;
  /** GitHub path to the test source directory. */
  sourcePath: string;
  /** Result per platform. */
  macos: MatrixCell;
  /** Result per platform. */
  linux: MatrixCell;
}

/**
 * Adversarial test results from `tests/adversarial/`.
 *
 * Each test uses a compiled Go binary that bypasses Layer 1 entirely —
 * direct syscalls, no JSON-RPC. This is the realistic adversary model.
 */
export const ADVERSARIAL_TESTS: AdversarialTest[] = [
  {
    id: "path_traversal",
    name: "Path Traversal",
    description: 'Sends "path": "../../etc/passwd" in a JSON-RPC call',
    sourcePath: "tests/adversarial/path_traversal",
    macos: { status: "blocked", layer: "Layer 1 — path traversal detection" },
    linux: { status: "blocked", layer: "Layer 1 — path traversal detection" },
  },
  {
    id: "read_passwd",
    name: "Read /etc/passwd",
    description: 'Sends "path": "/etc/passwd" — absolute path, not declared',
    sourcePath: "tests/adversarial/read_passwd",
    macos: { status: "blocked", layer: "Layer 1 — path not in allowlist" },
    linux: { status: "blocked", layer: "Layer 1 — path not in allowlist" },
  },
  {
    id: "call_forbidden",
    name: "Call Forbidden Host",
    description: "Sends a URL to an undeclared host",
    sourcePath: "tests/adversarial/call_forbidden",
    macos: { status: "blocked", layer: "Layer 1 — host not in network.allow" },
    linux: { status: "blocked", layer: "Layer 1 — host not in network.allow" },
  },
  {
    id: "call_metadata",
    name: "Call Metadata Service",
    description: "Sends http://169.254.169.254/latest/meta-data/",
    sourcePath: "tests/adversarial/call_metadata",
    macos: { status: "blocked", layer: "Layer 1 — deny_metadata: true" },
    linux: { status: "blocked", layer: "Layer 1 — deny_metadata: true" },
  },
  {
    id: "leak_secret",
    name: "Leak Secret in Output",
    description: "Tool response includes a fake AWS access key",
    sourcePath: "tests/adversarial/leak_secret",
    macos: { status: "blocked", layer: "Layer 1 — output secret scanning" },
    linux: { status: "blocked", layer: "Layer 1 — output secret scanning" },
  },
  {
    id: "spawn_child",
    name: "Spawn Child Process",
    description: 'Tool calls execvp("/bin/sh") directly',
    sourcePath: "tests/adversarial/spawn_child",
    macos: { status: "blocked", layer: "Layer 2 — Seatbelt process-exec deny" },
    linux: { status: "informational", layer: "Seccomp — kernel version dependent" },
  },
];

/** GitHub repo base URL for linking to test source. */
export const REPO_BASE_URL = "https://github.com/otomus/mcparmor/tree/main";
