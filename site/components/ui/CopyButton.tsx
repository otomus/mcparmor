"use client";

import { useState, useCallback, type ReactNode } from "react";

/** Duration to show "Copied" feedback before reverting. */
const FEEDBACK_DURATION_MS = 2000;

interface CopyButtonProps {
  /** Text to copy to clipboard. */
  text: string;
  /** Additional CSS classes. */
  className?: string;
}

/**
 * Button that copies text to the clipboard and shows brief feedback.
 *
 * Displays "Copy" normally and "Copied ✓" for 2 seconds after a click.
 * The feedback color transitions to `--color-blocked` (green).
 */
export function CopyButton({ text, className = "" }: CopyButtonProps): ReactNode {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), FEEDBACK_DURATION_MS);
    } catch {
      // Clipboard API unavailable — silently fail.
    }
  }, [text]);

  return (
    <button
      type="button"
      onClick={handleCopy}
      aria-label={copied ? "Copied to clipboard" : "Copy to clipboard"}
      className={`copy-feedback text-sm font-medium transition-colors cursor-pointer ${className}`}
      data-copied={copied ? "true" : undefined}
      style={{ color: copied ? "var(--color-blocked)" : "var(--color-text-secondary)" }}
    >
      {copied ? "Copied ✓" : "Copy"}
    </button>
  );
}
