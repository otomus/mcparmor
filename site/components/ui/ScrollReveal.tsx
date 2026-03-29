"use client";

import { useEffect, useRef, type ReactNode } from "react";

interface ScrollRevealProps {
  /** Content to reveal on scroll intersection. */
  children: ReactNode;
  /** Intersection Observer visibility threshold (0–1). */
  threshold?: number;
  /** Extra delay in ms before the reveal transition starts. */
  delay?: number;
  /** Direction variant: default (up), "left", "right", or "scale". */
  direction?: "up" | "left" | "right" | "scale";
  /** Additional CSS classes. */
  className?: string;
}

/**
 * Wraps children in a scroll-triggered reveal animation.
 *
 * Sets `data-reveal` on mount and `data-revealed` when the element
 * enters the viewport. CSS in `animations.css` handles the transition.
 */
export function ScrollReveal({
  children,
  threshold = 0.2,
  delay = 0,
  direction = "up",
  className = "",
}: ScrollRevealProps): ReactNode {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setTimeout(() => el.setAttribute("data-revealed", ""), delay);
          observer.unobserve(el);
        }
      },
      { threshold },
    );

    observer.observe(el);
    return () => observer.disconnect();
  }, [threshold, delay]);

  const revealValue = direction === "up" ? "" : direction;

  return (
    <div ref={ref} data-reveal={revealValue} className={className}>
      {children}
    </div>
  );
}
