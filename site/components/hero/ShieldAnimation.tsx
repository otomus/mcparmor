"use client";

import { useEffect, useRef, type ReactNode } from "react";

/**
 * Hero Variant B — "The Shield" SVG animation.
 *
 * Tool bubbles with approaching threat lines. The armor broker
 * materializes and blocks the threats with a green flash.
 */
export function ShieldAnimation(): ReactNode {
  const ref = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const svg = ref.current;
    if (!svg) return;

    const timeout = setTimeout(() => {
      svg.classList.add("animate-shield");
    }, 300);

    return () => clearTimeout(timeout);
  }, []);

  return (
    <svg
      ref={ref}
      viewBox="0 0 600 200"
      className="w-full max-w-xl mx-auto mt-8"
      role="img"
      aria-label="MCP Armor blocks threats from reaching MCP tools"
    >
      {/* Threat lines */}
      <line x1="0" y1="50" x2="280" y2="50" className="threat-line" stroke="var(--color-allowed)" strokeWidth="1.5" strokeDasharray="4 3" style={{ opacity: 0 }} />
      <line x1="0" y1="100" x2="280" y2="100" className="threat-line" stroke="var(--color-allowed)" strokeWidth="1.5" strokeDasharray="4 3" style={{ opacity: 0 }} />
      <line x1="0" y1="150" x2="280" y2="150" className="threat-line" stroke="var(--color-allowed)" strokeWidth="1.5" strokeDasharray="4 3" style={{ opacity: 0 }} />

      {/* Threat labels */}
      <text x="10" y="44" fontSize="9" fill="var(--color-allowed)" className="threat-label" style={{ opacity: 0 }}>/etc/passwd</text>
      <text x="10" y="94" fontSize="9" fill="var(--color-allowed)" className="threat-label" style={{ opacity: 0 }}>evil.com:80</text>
      <text x="10" y="144" fontSize="9" fill="var(--color-allowed)" className="threat-label" style={{ opacity: 0 }}>exec(/bin/sh)</text>

      {/* Broker membrane */}
      <rect x="278" y="20" width="4" height="160" rx="2" fill="var(--color-accent)" className="broker-membrane" style={{ opacity: 0 }} />

      {/* Block flash indicators */}
      <circle cx="280" cy="50" r="6" fill="var(--color-blocked)" className="block-flash" style={{ opacity: 0 }} />
      <circle cx="280" cy="100" r="6" fill="var(--color-blocked)" className="block-flash" style={{ opacity: 0 }} />
      <circle cx="280" cy="150" r="6" fill="var(--color-blocked)" className="block-flash" style={{ opacity: 0 }} />

      {/* Tool bubbles */}
      <g className="tool-bubble" style={{ opacity: 0 }}>
        <rect x="320" y="25" width="140" height="40" rx="20" fill="var(--color-bg-muted)" stroke="var(--color-border-strong)" strokeWidth="1.5" />
        <text x="390" y="50" textAnchor="middle" fontSize="11" fill="var(--color-text-primary)">github_server</text>
      </g>
      <g className="tool-bubble" style={{ opacity: 0 }}>
        <rect x="320" y="80" width="140" height="40" rx="20" fill="var(--color-bg-muted)" stroke="var(--color-border-strong)" strokeWidth="1.5" />
        <text x="390" y="105" textAnchor="middle" fontSize="11" fill="var(--color-text-primary)">filesystem</text>
      </g>
      <g className="tool-bubble" style={{ opacity: 0 }}>
        <rect x="320" y="135" width="140" height="40" rx="20" fill="var(--color-bg-muted)" stroke="var(--color-border-strong)" strokeWidth="1.5" />
        <text x="390" y="160" textAnchor="middle" fontSize="11" fill="var(--color-text-primary)">unknown_tool</text>
      </g>

      {/* Broker label */}
      <text x="280" y="12" textAnchor="middle" fontSize="10" fontWeight="600" fill="var(--color-accent)" className="broker-label" style={{ opacity: 0 }}>MCP Armor Broker</text>

      <style>{`
        .animate-shield .tool-bubble {
          animation: fadeIn 400ms 0ms ease forwards;
        }
        .animate-shield .threat-line {
          animation: fadeIn 300ms 400ms ease forwards;
        }
        .animate-shield .threat-label {
          animation: fadeIn 300ms 500ms ease forwards;
        }
        .animate-shield .broker-membrane {
          animation: fadeIn 300ms 800ms ease forwards;
        }
        .animate-shield .block-flash {
          animation: blockedFlash 600ms 1000ms ease forwards;
        }
        .animate-shield .broker-label {
          animation: fadeIn 400ms 1200ms ease forwards;
        }
        @keyframes fadeIn {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        @keyframes blockedFlash {
          0% { opacity: 0; }
          30% { opacity: 1; }
          100% { opacity: 0.6; }
        }
        @media (prefers-reduced-motion: reduce) {
          .animate-shield .tool-bubble,
          .animate-shield .threat-line,
          .animate-shield .threat-label,
          .animate-shield .broker-membrane,
          .animate-shield .block-flash,
          .animate-shield .broker-label {
            opacity: 1 !important;
            animation: none !important;
          }
        }
      `}</style>
    </svg>
  );
}
