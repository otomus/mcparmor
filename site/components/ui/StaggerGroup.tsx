"use client";

import {
  useEffect,
  useRef,
  Children,
  type ReactNode,
} from "react";

interface StaggerGroupProps {
  /** Items to reveal with staggered timing. */
  children: ReactNode;
  /** Delay between each child in ms. */
  interval?: number;
  /** Intersection Observer visibility threshold (0–1). */
  threshold?: number;
  /** Additional CSS classes on the wrapper. */
  className?: string;
}

/**
 * Reveals children with staggered timing on scroll intersection.
 *
 * Each child gets a `transition-delay` based on its index × interval.
 * The group's `data-revealed` attribute triggers all children via CSS.
 */
export function StaggerGroup({
  children,
  interval = 60,
  threshold = 0.2,
  className = "",
}: StaggerGroupProps): ReactNode {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          el.setAttribute("data-revealed", "");
          observer.unobserve(el);
        }
      },
      { threshold },
    );

    observer.observe(el);
    return () => observer.disconnect();
  }, [threshold]);

  return (
    <div ref={ref} data-stagger="" className={className}>
      {Children.map(children, (child, i) => (
        <div style={{ transitionDelay: `${i * interval}ms` }}>{child}</div>
      ))}
    </div>
  );
}
