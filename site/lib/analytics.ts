/**
 * Lightweight analytics event helpers for Plausible.
 *
 * All events are fire-and-forget. If Plausible is not loaded (e.g. in
 * development), calls are silently ignored.
 */

interface PlausibleWindow extends Window {
  plausible?: (event: string, options?: { props?: Record<string, string> }) => void;
}

/** Fire a custom Plausible event. */
export function trackEvent(name: string, props?: Record<string, string>): void {
  const w = window as PlausibleWindow;
  if (typeof w.plausible === "function") {
    w.plausible(name, props ? { props } : undefined);
  }
}

/** Track which hero A/B variant was shown. */
export function trackHeroVariant(variant: string): void {
  trackEvent("hero_variant", { variant });
}
