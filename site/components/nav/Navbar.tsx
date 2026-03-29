"use client";

import { useState, type ReactNode } from "react";
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
    <a href="/" className="text-lg font-semibold" style={{ fontFamily: "var(--font-body)" }} aria-label="MCP Armor home">
      MCP ARM
      <span style={{ color: "var(--color-accent)" }} aria-hidden="true">⬡</span>R
    </a>
  );
}

function DesktopLinks(): ReactNode {
  return (
    <div className="hidden md:flex items-center gap-6">
      <a href="/docs" className="text-sm" style={{ color: "var(--color-text-secondary)" }}>
        Docs
      </a>
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
        brew install mcparmor
      </span>
      <CopyButton text="brew install mcparmor" />
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
      <a href="/docs" className="text-sm" style={{ color: "var(--color-text-secondary)" }} onClick={onClose}>
        Docs
      </a>
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
