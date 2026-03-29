import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";
import { PLATFORMS, type PlatformCard } from "@/lib/data/platforms";

/** Platform guarantee cards — macOS, Linux, Windows. */
export function PlatformCards(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-5xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center mb-12"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            Platform Guarantees
          </h2>
        </ScrollReveal>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {PLATFORMS.map((platform, i) => (
            <ScrollReveal key={platform.name} direction="scale" delay={i * 80}>
              <Card platform={platform} />
            </ScrollReveal>
          ))}
        </div>
      </div>
    </section>
  );
}

function Card({ platform }: { platform: PlatformCard }): ReactNode {
  return (
    <div
      className="rounded-xl p-6 border h-full"
      style={{ borderColor: "var(--color-border-strong)" }}
    >
      <div className="flex items-center gap-2 mb-4">
        <h3 className="font-semibold" style={{ fontSize: "var(--text-h2)" }}>
          {platform.name}
        </h3>
        <BadgeIcon badge={platform.badge} />
      </div>

      <p
        className="text-sm mb-4"
        style={{ color: "var(--color-text-secondary)" }}
      >
        {platform.enforcement} via {platform.mechanism}
      </p>

      <ul className="flex flex-col gap-1">
        {platform.capabilities.map((cap) => (
          <li
            key={cap}
            className="text-sm flex items-center gap-2"
            style={{ color: "var(--color-text-secondary)" }}
          >
            <span style={{ color: "var(--color-blocked)" }}>✓</span>
            {cap}
          </li>
        ))}
      </ul>
    </div>
  );
}

function BadgeIcon({ badge }: { badge: PlatformCard["badge"] }): ReactNode {
  const colors: Record<string, { bg: string; fg: string }> = {
    Full: { bg: "var(--color-blocked-bg)", fg: "var(--color-blocked)" },
    Kernel: { bg: "var(--color-blocked-bg)", fg: "var(--color-blocked)" },
    Protocol: { bg: "var(--color-allowed-bg)", fg: "var(--color-allowed)" },
  };

  const { bg, fg } = colors[badge] ?? colors.Protocol;

  return (
    <span
      className="text-xs font-medium px-2 py-0.5 rounded-full"
      style={{ backgroundColor: bg, color: fg }}
    >
      {badge}
    </span>
  );
}
