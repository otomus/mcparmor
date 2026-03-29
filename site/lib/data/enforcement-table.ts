/** A row in the enforcement mechanism table. */
export interface EnforcementRow {
  /** Capability being protected. */
  capability: string;
  /** How it's enforced (mechanism name). */
  mechanism: string;
  /** Reliability level. */
  reliability: "hard" | "soft";
  /** Platform(s) where this enforcement is active. */
  platform: string;
}

/**
 * Two-layer enforcement table from PRODUCT.md.
 *
 * Each row maps a capability to its enforcement mechanism, reliability
 * rating, and platform availability.
 */
export const ENFORCEMENT_TABLE: EnforcementRow[] = [
  {
    capability: "Env var restriction",
    mechanism: "Strips env at spawn",
    reliability: "hard",
    platform: "All",
  },
  {
    capability: "Param path/URL validation",
    mechanism: "Inspects JSON-RPC params before forwarding",
    reliability: "hard",
    platform: "All",
  },
  {
    capability: "Response secret scanning",
    mechanism: "Regex on every response",
    reliability: "hard",
    platform: "All",
  },
  {
    capability: "Timeout",
    mechanism: "SIGTERM/SIGKILL on deadline",
    reliability: "hard",
    platform: "All",
  },
  {
    capability: "Output size limit",
    mechanism: "Truncate at max_bytes",
    reliability: "hard",
    platform: "All",
  },
  {
    capability: "Filesystem isolation",
    mechanism: "Landlock (Linux 5.13+) / Seatbelt (macOS)",
    reliability: "hard",
    platform: "Linux 5.13+, macOS",
  },
  {
    capability: "Spawn blocking",
    mechanism: "Seccomp (Linux 3.5+) / Seatbelt (macOS)",
    reliability: "hard",
    platform: "Linux 3.5+, macOS",
  },
  {
    capability: "Network by hostname",
    mechanism: "Seatbelt (macOS)",
    reliability: "hard",
    platform: "macOS only",
  },
  {
    capability: "Network by TCP port",
    mechanism: "Landlock (Linux 6.7+)",
    reliability: "hard",
    platform: "Linux 6.7+",
  },
];
