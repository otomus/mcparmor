"use client";

import { useEffect, useState, type ReactNode } from "react";

/** Fallback star count shown before the API responds or on failure. */
const FALLBACK_STARS = "—";

/**
 * Live GitHub star count badge.
 *
 * Fetches the star count from the GitHub API on mount. Falls back to
 * a dash if the API is unavailable so the UI never breaks.
 */
export function GitHubStars(): ReactNode {
  const [stars, setStars] = useState(FALLBACK_STARS);

  useEffect(() => {
    fetch("https://api.github.com/repos/otomus/mcparmor")
      .then((res) => res.json())
      .then((data) => {
        if (typeof data.stargazers_count === "number") {
          setStars(formatCount(data.stargazers_count));
        }
      })
      .catch(() => {
        // API unavailable — keep fallback.
      });
  }, []);

  return (
    <span
      className="text-xs px-2 py-0.5 rounded-full"
      style={{
        backgroundColor: "var(--color-bg-muted)",
        color: "var(--color-text-secondary)",
      }}
    >
      ★ {stars}
    </span>
  );
}

/** Format a number with K suffix for thousands. */
function formatCount(count: number): string {
  if (count >= 1000) {
    return `${(count / 1000).toFixed(1)}k`;
  }
  return String(count);
}
