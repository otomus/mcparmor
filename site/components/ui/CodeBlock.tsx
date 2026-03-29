import { type ReactNode } from "react";
import { CopyButton } from "./CopyButton";

interface CodeBlockProps {
  /** The source code to display. */
  code: string;
  /** Language identifier for syntax highlighting context (display only). */
  lang?: string;
  /** Optional filename shown above the code. */
  filename?: string;
}

/**
 * Syntax-highlighted code block with a copy button.
 *
 * Renders code in a `<pre>` with monospace font and muted background.
 * A CopyButton sits in the top-right corner.
 */
export function CodeBlock({
  code,
  lang,
  filename,
}: CodeBlockProps): ReactNode {
  return (
    <div
      className="relative rounded-lg overflow-hidden"
      style={{ backgroundColor: "var(--color-bg-muted)" }}
    >
      {filename && (
        <div
          className="px-4 py-2 text-sm font-medium border-b flex items-center justify-between"
          style={{
            color: "var(--color-text-secondary)",
            borderColor: "var(--color-border)",
          }}
        >
          <span>{filename}</span>
          <CopyButton text={code} />
        </div>
      )}
      {!filename && (
        <div className="absolute top-2 right-2 z-10">
          <CopyButton text={code} />
        </div>
      )}
      <pre
        className="overflow-x-auto p-4"
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-mono)",
          lineHeight: "var(--lh-mono)",
        }}
      >
        <code data-lang={lang}>{code}</code>
      </pre>
    </div>
  );
}
