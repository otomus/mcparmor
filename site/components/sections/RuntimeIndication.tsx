import type { ReactNode } from "react";
import { ScrollReveal } from "@/components/ui/ScrollReveal";

/**
 * Runtime indication section — shows how MCP Armor surfaces protection
 * status directly in the host UI (tool descriptions, stderr banners,
 * and block notifications).
 */
export function RuntimeIndication(): ReactNode {
  return (
    <section className="py-24 px-4">
      <div className="max-w-5xl mx-auto">
        <ScrollReveal>
          <h2
            className="text-center mb-4"
            style={{
              fontFamily: "var(--font-display)",
              fontSize: "var(--text-h1)",
              lineHeight: "var(--lh-h1)",
            }}
          >
            You See It Working.
          </h2>
          <p
            className="text-center mb-16 max-w-2xl mx-auto"
            style={{
              color: "var(--color-text-secondary)",
              lineHeight: "var(--lh-body)",
            }}
          >
            No blind trust. MCP Armor shows protection status directly in your
            host UI — and tells you the moment it blocks something.
          </p>
        </ScrollReveal>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
          <ScrollReveal delay={0}>
            <IndicatorCard
              label="Tool Discovery"
              title="Shield in Tool List"
              description="Every tool&rsquo;s description is annotated with a shield indicator so you know it&rsquo;s protected before you use it."
              example={
                <ToolListExample />
              }
            />
          </ScrollReveal>
          <ScrollReveal delay={100}>
            <IndicatorCard
              label="Startup"
              title="Protection Banner"
              description="When a tool launches, the broker prints its enforcement status — profile, sandbox mechanism, and active layers."
              example={
                <StderrExample
                  lines={[
                    "[mcparmor] server-github protected | profile: network | layers: protocol+seatbelt",
                  ]}
                />
              }
            />
          </ScrollReveal>
          <ScrollReveal delay={200}>
            <IndicatorCard
              label="Enforcement"
              title="Block Notification"
              description="When a tool tries to access something outside its declared capabilities, the block is visible immediately."
              example={
                <StderrExample
                  lines={[
                    "[mcparmor] BLOCKED tools/call \u2014 Path /etc/passwd is not in the filesystem allowlist",
                  ]}
                  variant="deny"
                />
              }
            />
          </ScrollReveal>
        </div>
      </div>
    </section>
  );
}

/** Card showing one type of runtime indicator. */
function IndicatorCard({
  label,
  title,
  description,
  example,
}: {
  label: string;
  title: string;
  description: string;
  example: ReactNode;
}): ReactNode {
  return (
    <div
      className="rounded-lg p-6 flex flex-col gap-4 h-full"
      style={{ backgroundColor: "var(--color-bg-muted)" }}
    >
      <span
        className="text-xs font-semibold uppercase tracking-wider"
        style={{ color: "var(--color-accent)" }}
      >
        {label}
      </span>
      <h3 className="font-semibold" style={{ fontSize: "var(--text-h3)" }}>
        {title}
      </h3>
      <p
        className="text-sm flex-1"
        style={{
          color: "var(--color-text-secondary)",
          lineHeight: "var(--lh-body)",
        }}
        dangerouslySetInnerHTML={{ __html: description }}
      />
      <div className="mt-auto">{example}</div>
    </div>
  );
}

/** Mock tool list showing shield annotation in tool descriptions. */
function ToolListExample(): ReactNode {
  return (
    <div
      className="rounded text-xs p-3 flex flex-col gap-2"
      style={{
        backgroundColor: "var(--color-bg-inset)",
        fontFamily: "var(--font-mono)",
      }}
    >
      <ToolRow name="read_file" desc="Read file contents" armored />
      <ToolRow name="search_code" desc="Search codebase" armored />
      <ToolRow name="run_query" desc="Execute SQL query" armored />
    </div>
  );
}

/** Single row in the mock tool list. */
function ToolRow({
  name,
  desc,
  armored,
}: {
  name: string;
  desc: string;
  armored: boolean;
}): ReactNode {
  return (
    <div className="flex items-baseline gap-2">
      <span style={{ color: "var(--color-text-primary)" }}>{name}</span>
      <span style={{ color: "var(--color-text-tertiary)" }}>
        {desc}
        {armored && (
          <span style={{ color: "var(--color-accent)" }}>
            {" "}
            [&#x1F6E1; MCP Armor]
          </span>
        )}
      </span>
    </div>
  );
}

/** Mock stderr output. */
function StderrExample({
  lines,
  variant = "allow",
}: {
  lines: string[];
  variant?: "allow" | "deny";
}): ReactNode {
  const color =
    variant === "deny" ? "var(--color-error, #ef4444)" : "var(--color-accent)";

  return (
    <div
      className="rounded text-xs p-3 overflow-x-auto"
      style={{
        backgroundColor: "var(--color-bg-inset)",
        fontFamily: "var(--font-mono)",
        color,
      }}
    >
      {lines.map((line, i) => (
        <div key={i}>{line}</div>
      ))}
    </div>
  );
}
