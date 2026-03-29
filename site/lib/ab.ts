/** A/B test variants for the hero section. */
export type HeroVariant = "chain" | "shield";

const STORAGE_KEY = "mcparmor_hero_variant";

/**
 * Get or assign the hero A/B test variant.
 *
 * On first visit, randomly picks "chain" or "shield" (50/50) and
 * stores the choice in localStorage. Subsequent visits return the
 * same variant for consistency.
 *
 * @returns The assigned variant, or "chain" as fallback when
 *          localStorage is unavailable.
 */
export function getHeroVariant(): HeroVariant {
  if (typeof window === "undefined") return "chain";

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "chain" || stored === "shield") return stored;

    const variant: HeroVariant = Math.random() < 0.5 ? "chain" : "shield";
    localStorage.setItem(STORAGE_KEY, variant);
    return variant;
  } catch {
    return "chain";
  }
}
