"use client";

import { useEffect, useRef, type ReactNode } from "react";

/**
 * Hero Variant A — "The Chain" SVG animation.
 *
 * Three chain links (MCP Host → gap → MCP Tool) with the armor link
 * dropping in to fill the gap. Triggered on mount.
 */
export function ChainAnimation(): ReactNode {
  const ref = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const svg = ref.current;
    if (!svg) return;

    const timeout = setTimeout(() => {
      svg.classList.add("animate-chain");
    }, 300);

    return () => clearTimeout(timeout);
  }, []);

  return (
    <svg
      ref={ref}
      viewBox="0 0 600 120"
      className="w-full max-w-xl mx-auto mt-8"
      role="img"
      aria-label="MCP Armor fills the missing link between MCP Host and MCP Tool"
    >
      {/* MCP Host link */}
      <g className="chain-link-1" style={{ opacity: 0 }}>
        <rect x="10" y="30" width="150" height="60" rx="12" fill="var(--color-bg-muted)" stroke="var(--color-border-strong)" strokeWidth="2" />
        <text x="85" y="65" textAnchor="middle" fontSize="13" fontWeight="600" fill="var(--color-text-primary)">MCP Host</text>
      </g>

      {/* Gap (pulsing) */}
      <g className="chain-gap" style={{ opacity: 0 }}>
        <rect x="195" y="30" width="120" height="60" rx="12" fill="none" stroke="var(--color-allowed)" strokeWidth="2" strokeDasharray="6 4">
          <animate attributeName="opacity" values="0.4;1;0.4" dur="2s" repeatCount="indefinite" />
        </rect>
        <text x="255" y="65" textAnchor="middle" fontSize="11" fill="var(--color-allowed)">?</text>
      </g>

      {/* MCP Tool link */}
      <g className="chain-link-3" style={{ opacity: 0 }}>
        <rect x="350" y="30" width="150" height="60" rx="12" fill="var(--color-bg-muted)" stroke="var(--color-border-strong)" strokeWidth="2" />
        <text x="425" y="65" textAnchor="middle" fontSize="13" fontWeight="600" fill="var(--color-text-primary)">MCP Tool</text>
      </g>

      {/* MCP Armor link (drops in) */}
      <g className="chain-armor" style={{ opacity: 0, transform: "translateY(-30px)" }}>
        <rect x="195" y="30" width="120" height="60" rx="12" fill="var(--color-accent-subtle)" stroke="var(--color-accent)" strokeWidth="2" />
        <text x="255" y="58" textAnchor="middle" fontSize="11" fontWeight="600" fill="var(--color-accent)">MCP</text>
        <text x="255" y="74" textAnchor="middle" fontSize="11" fontWeight="600" fill="var(--color-accent)">ARM⬡R</text>
      </g>

      {/* Connecting lines */}
      <line x1="160" y1="60" x2="195" y2="60" stroke="var(--color-border-strong)" strokeWidth="2" className="chain-connector" style={{ opacity: 0 }} />
      <line x1="315" y1="60" x2="350" y2="60" stroke="var(--color-border-strong)" strokeWidth="2" className="chain-connector" style={{ opacity: 0 }} />

      <style>{`
        .animate-chain .chain-link-1 {
          animation: fadeSlideIn 400ms 0ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
        }
        .animate-chain .chain-gap {
          animation: fadeSlideIn 400ms 200ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
        }
        .animate-chain .chain-link-3 {
          animation: fadeSlideIn 400ms 400ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
        }
        .animate-chain .chain-connector {
          animation: fadeSlideIn 300ms 600ms ease forwards;
        }
        .animate-chain .chain-armor {
          animation: dropIn 500ms 1000ms cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
        }
        @keyframes fadeSlideIn {
          from { opacity: 0; transform: translateX(-10px); }
          to { opacity: 1; transform: translateX(0); }
        }
        @keyframes dropIn {
          from { opacity: 0; transform: translateY(-30px); }
          to { opacity: 1; transform: translateY(0); }
        }
        @media (prefers-reduced-motion: reduce) {
          .animate-chain .chain-link-1,
          .animate-chain .chain-gap,
          .animate-chain .chain-link-3,
          .animate-chain .chain-connector,
          .animate-chain .chain-armor {
            opacity: 1 !important;
            transform: none !important;
            animation: none !important;
          }
        }
      `}</style>
    </svg>
  );
}
