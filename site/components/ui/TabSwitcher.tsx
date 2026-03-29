"use client";

import {
  useState,
  useCallback,
  type ReactNode,
  type KeyboardEvent,
} from "react";

interface Tab {
  /** Unique identifier for the tab. */
  id: string;
  /** Display label shown in the tab bar. */
  label: string;
  /** Content rendered when this tab is active. */
  content: ReactNode;
}

interface TabSwitcherProps {
  /** Tabs to render. At least one required. */
  tabs: Tab[];
  /** Additional CSS classes on the wrapper. */
  className?: string;
}

/**
 * Accessible tab component with fade transitions.
 *
 * Supports keyboard navigation (arrow keys) and proper ARIA roles.
 * Content fades in over 150ms on tab switch.
 */
export function TabSwitcher({ tabs, className = "" }: TabSwitcherProps): ReactNode {
  const [activeId, setActiveId] = useState(tabs[0]?.id ?? "");

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const currentIndex = tabs.findIndex((t) => t.id === activeId);
      if (e.key === "ArrowRight") {
        const next = (currentIndex + 1) % tabs.length;
        setActiveId(tabs[next].id);
      } else if (e.key === "ArrowLeft") {
        const prev = (currentIndex - 1 + tabs.length) % tabs.length;
        setActiveId(tabs[prev].id);
      }
    },
    [activeId, tabs],
  );

  const activeTab = tabs.find((t) => t.id === activeId);

  return (
    <div className={className}>
      <div role="tablist" className="flex gap-1 mb-4" onKeyDown={handleKeyDown}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            id={`tab-${tab.id}`}
            aria-selected={tab.id === activeId}
            aria-controls={`panel-${tab.id}`}
            tabIndex={tab.id === activeId ? 0 : -1}
            onClick={() => setActiveId(tab.id)}
            className="px-4 py-2 text-sm font-medium rounded-full transition-colors cursor-pointer"
            style={{
              backgroundColor:
                tab.id === activeId ? "var(--color-accent)" : "var(--color-bg-muted)",
              color: tab.id === activeId ? "#fff" : "var(--color-text-secondary)",
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div
        key={activeId}
        id={`panel-${activeId}`}
        role="tabpanel"
        aria-labelledby={`tab-${activeId}`}
        className="animate-fade-in"
        style={{ animation: "fadeIn 150ms ease" }}
      >
        {activeTab?.content}
      </div>
    </div>
  );
}
