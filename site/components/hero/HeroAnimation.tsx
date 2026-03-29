"use client";

import { useEffect, useState, type ReactNode } from "react";
import { getHeroVariant, type HeroVariant } from "@/lib/ab";
import { trackHeroVariant } from "@/lib/analytics";
import { ChainAnimation } from "./ChainAnimation";
import { ShieldAnimation } from "./ShieldAnimation";

/**
 * A/B test wrapper for the hero animation.
 *
 * Picks a variant on mount (from localStorage or random assignment)
 * and fires an analytics event. Renders the corresponding animation.
 */
export function HeroAnimation(): ReactNode {
  const [variant, setVariant] = useState<HeroVariant | null>(null);

  useEffect(() => {
    const v = getHeroVariant();
    setVariant(v);
    trackHeroVariant(v);
  }, []);

  if (!variant) return null;

  return variant === "chain" ? <ChainAnimation /> : <ShieldAnimation />;
}
