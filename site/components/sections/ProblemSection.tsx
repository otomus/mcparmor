import type { ReactNode } from "react";
import { StaggerGroup } from "@/components/ui/StaggerGroup";

/**
 * Problem statement section — three centered lines in large type.
 *
 * Typography does the work. No images, no icons.
 */
export function ProblemSection(): ReactNode {
  const statements = [
    "Every MCP tool runs with your permissions.",
    "Your SSH keys. Your AWS credentials. Your entire home directory.",
    "The protocol has no capability model. MCP Armor adds one.",
  ];

  return (
    <section
      className="py-24 px-4"
      style={{ backgroundColor: "var(--color-bg-subtle)" }}
    >
      <StaggerGroup
        interval={150}
        className="max-w-3xl mx-auto flex flex-col gap-8 text-center"
      >
        {statements.map((text) => (
          <p
            key={text}
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
              color: "var(--color-text-primary)",
            }}
          >
            {text}
          </p>
        ))}
      </StaggerGroup>
    </section>
  );
}
