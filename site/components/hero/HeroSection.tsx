import type { ReactNode } from "react";
import { InstallCTA } from "./InstallCTA";
import { HeroAnimation } from "./HeroAnimation";

/**
 * Hero section with headline, subtext, install CTA, and A/B animation.
 */
export function HeroSection(): ReactNode {
  return (
    <section className="pt-32 pb-16 px-4">
      <div className="max-w-4xl mx-auto text-center">
        <h1
          style={{
            fontFamily: "var(--font-display)",
            fontSize: "var(--text-display)",
            lineHeight: "var(--lh-display)",
            color: "var(--color-text-primary)",
          }}
        >
          MCP made tools composable.
          <br />
          It didn&apos;t make them safe.
        </h1>

        <p
          className="mt-6 max-w-2xl mx-auto"
          style={{
            fontSize: "var(--text-body-lg)",
            lineHeight: "var(--lh-body-lg)",
            color: "var(--color-text-secondary)",
          }}
        >
          MCP Armor adds the missing layer — capability boundaries enforced at runtime.
          Not by convention. By the OS where possible, by protocol where not.
        </p>

        <InstallCTA />
        <HeroAnimation />
      </div>
    </section>
  );
}
