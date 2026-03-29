/** Platform enforcement guarantee card. */
export interface PlatformCard {
  /** Platform name and version. */
  name: string;
  /** Enforcement level badge label. */
  badge: "Full" | "Kernel" | "Protocol";
  /** What enforcement primitives are active. */
  enforcement: string;
  /** The sandbox mechanism name. */
  mechanism: string;
  /** Specific capabilities enforced. */
  capabilities: string[];
}

/** Platform guarantee cards from DESIGN.md §7. */
export const PLATFORMS: PlatformCard[] = [
  {
    name: "macOS 12+",
    badge: "Full",
    enforcement: "Full enforcement",
    mechanism: "Seatbelt",
    capabilities: [
      "Filesystem isolation",
      "Spawn blocking",
      "Hostname-level network",
      "Env stripping",
      "Secret scanning",
    ],
  },
  {
    name: "Linux 5.13+ / 6.7+",
    badge: "Kernel",
    enforcement: "Kernel enforcement",
    mechanism: "Landlock + Seccomp",
    capabilities: [
      "Filesystem isolation (5.13+)",
      "Spawn blocking (Seccomp)",
      "TCP port network (6.7+)",
      "Env stripping",
      "Secret scanning",
    ],
  },
  {
    name: "Windows",
    badge: "Protocol",
    enforcement: "Protocol layer",
    mechanism: "Broker only (v1)",
    capabilities: [
      "Env stripping",
      "Param validation",
      "Secret scanning",
      "Timeout + size limit",
    ],
  },
];
