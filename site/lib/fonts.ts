import { DM_Serif_Display, Inter, JetBrains_Mono } from "next/font/google";

/** Display headings — large hero text, section titles. */
export const dmSerifDisplay = DM_Serif_Display({
  weight: "400",
  subsets: ["latin"],
  display: "swap",
  variable: "--font-display",
});

/** UI headings, nav, labels, body text. */
export const inter = Inter({
  subsets: ["latin"],
  display: "swap",
  variable: "--font-body",
});

/** Code blocks, terminal output, install commands. */
export const jetbrainsMono = JetBrains_Mono({
  subsets: ["latin"],
  display: "swap",
  variable: "--font-mono",
});
