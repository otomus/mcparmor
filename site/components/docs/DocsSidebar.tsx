"use client";

import { useState, type ReactNode } from "react";

interface SidebarSection {
  title: string;
  links: { href: string; label: string }[];
}

const SECTIONS: SidebarSection[] = [
  {
    title: "Getting Started",
    links: [
      { href: "/docs/installation", label: "Installation" },
      { href: "/docs/quickstart", label: "Quick Start" },
    ],
  },
  {
    title: "Reference",
    links: [
      { href: "/docs/manifest", label: "armor.json Manifest" },
      { href: "/docs/cli", label: "CLI Commands" },
    ],
  },
  {
    title: "Security",
    links: [
      { href: "/docs/security", label: "Enforcement Model" },
    ],
  },
  {
    title: "Integrations",
    links: [
      { href: "/docs/integrations", label: "Host Integrations" },
    ],
  },
  {
    title: "Community",
    links: [
      { href: "/docs/profiles", label: "Community Profiles" },
    ],
  },
];

/** Docs sidebar navigation. Fixed on desktop, off-canvas drawer on mobile. */
export function DocsSidebar(): ReactNode {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        type="button"
        className="lg:hidden fixed top-20 left-4 z-40 p-2 rounded-md border"
        style={{
          backgroundColor: "var(--color-bg)",
          borderColor: "var(--color-border)",
        }}
        onClick={() => setOpen(!open)}
        aria-label="Toggle docs navigation"
        aria-expanded={open}
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
          <path d="M3 12h18M3 6h18M3 18h18" />
        </svg>
      </button>

      <aside
        className={`
          fixed top-16 left-0 h-[calc(100vh-4rem)] w-64 overflow-y-auto p-6 border-r z-30
          transition-transform lg:translate-x-0 lg:sticky lg:top-16
          ${open ? "translate-x-0" : "-translate-x-full"}
        `}
        style={{
          backgroundColor: "var(--color-bg)",
          borderColor: "var(--color-border)",
        }}
      >
        <nav aria-label="Documentation" className="flex flex-col gap-6">
          {SECTIONS.map((section) => (
            <div key={section.title}>
              <p
                className="text-xs font-semibold uppercase tracking-wider mb-2"
                style={{ color: "var(--color-text-tertiary)" }}
              >
                {section.title}
              </p>
              <ul className="flex flex-col gap-1">
                {section.links.map((link) => (
                  <li key={link.href}>
                    <a
                      href={link.href}
                      className="block text-sm py-1 hover:underline"
                      style={{ color: "var(--color-text-secondary)" }}
                      onClick={() => setOpen(false)}
                    >
                      {link.label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </nav>
      </aside>

      {open && (
        <div
          className="fixed inset-0 bg-black/20 z-20 lg:hidden"
          onClick={() => setOpen(false)}
          aria-hidden="true"
        />
      )}
    </>
  );
}
