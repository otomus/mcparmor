"use client";

import { useState, type ReactNode } from "react";
import Link from "next/link";
import { CopyButton } from "@/components/ui/CopyButton";
import { GitHubStars } from "./GitHubStars";

/**
 * Fixed top navigation bar.
 *
 * Desktop: logo left, links + install CTA right.
 * Mobile: logo + hamburger, full-width slide-down menu.
 */
export function Navbar(): ReactNode {
  const [menuOpen, setMenuOpen] = useState(false);

  return (
    <nav
      className="fixed top-0 left-0 right-0 z-50 border-b"
      aria-label="Main navigation"
      style={{
        backgroundColor: "var(--color-bg)",
        borderColor: "var(--color-border)",
      }}
    >
      <div className="max-w-6xl mx-auto px-4 h-16 flex items-center justify-between">
        <Logo />
        <DesktopLinks />
        <HamburgerButton open={menuOpen} onToggle={() => setMenuOpen(!menuOpen)} />
      </div>
      {menuOpen && <MobileMenu onClose={() => setMenuOpen(false)} />}
    </nav>
  );
}

function Logo(): ReactNode {
  return (
    <Link href="/" className="flex items-center gap-2 text-lg font-semibold" style={{ fontFamily: "var(--font-body)" }} aria-label="MCP Armor home">
      <span>
        MCP ARM
        <span style={{ color: "var(--color-accent)" }} aria-hidden="true">⬡</span>R
      </span>
      <span
        className="text-xs font-normal px-1.5 py-0.5 rounded"
        style={{
          color: "var(--color-accent)",
          backgroundColor: "var(--color-bg-muted)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {process.env.NEXT_PUBLIC_VERSION ?? "dev"}
      </span>
    </Link>
  );
}

function DesktopLinks(): ReactNode {
  return (
    <div className="hidden md:flex items-center gap-6">
      <Link href="/docs" className="text-sm" style={{ color: "var(--color-text-secondary)" }}>
        Docs
      </Link>
      <a
        href="https://github.com/otomus/mcparmor"
        className="text-sm flex items-center gap-2"
        style={{ color: "var(--color-text-secondary)" }}
        target="_blank"
        rel="noopener noreferrer"
      >
        GitHub
        <GitHubStars />
      </a>
      <InstallButton />
    </div>
  );
}

function InstallButton(): ReactNode {
  return (
    <div className="flex items-center gap-2">
      <span
        className="px-4 py-2 rounded-full text-sm font-medium text-white"
        style={{
          backgroundColor: "var(--color-accent)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-sm)",
        }}
      >
        brew install otomus-mcp-armor
      </span>
      <CopyButton text="brew tap otomus/mcparmor https://github.com/otomus/mcparmor && brew install otomus-mcp-armor" />
    </div>
  );
}

interface HamburgerButtonProps {
  open: boolean;
  onToggle: () => void;
}

function HamburgerButton({ open, onToggle }: HamburgerButtonProps): ReactNode {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="md:hidden p-2"
      aria-label={open ? "Close menu" : "Open menu"}
      aria-expanded={open}
    >
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
        {open ? (
          <path d="M6 6l12 12M6 18L18 6" />
        ) : (
          <path d="M3 12h18M3 6h18M3 18h18" />
        )}
      </svg>
    </button>
  );
}

function MobileMenu({ onClose }: { onClose: () => void }): ReactNode {
  return (
    <div
      className="md:hidden border-b p-4 flex flex-col gap-4"
      style={{ backgroundColor: "var(--color-bg)", borderColor: "var(--color-border)" }}
    >
      <Link href="/docs" className="text-sm" style={{ color: "var(--color-text-secondary)" }} onClick={onClose}>
        Docs
      </Link>
      <a
        href="https://github.com/otomus/mcparmor"
        className="text-sm"
        style={{ color: "var(--color-text-secondary)" }}
        target="_blank"
        rel="noopener noreferrer"
      >
        GitHub
      </a>
      <InstallButton />
    </div>
  );
}
