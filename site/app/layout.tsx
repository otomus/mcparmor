import type { Metadata } from "next";
import { dmSerifDisplay, inter, jetbrainsMono } from "@/lib/fonts";
import { Navbar } from "@/components/nav/Navbar";
import { Footer } from "@/components/nav/Footer";
import "@/lib/tokens.css";
import "@/lib/animations.css";
import "./globals.css";

export const metadata: Metadata = {
  title: "MCP Armor — Capability enforcement for MCP tools",
  description:
    "MCP made tools composable. It didn't make them safe. " +
    "MCP Armor adds the missing layer — enforced at the OS level, not by convention.",
  icons: {
    icon: "/favicon.svg",
    apple: "/favicon.svg",
  },
  openGraph: {
    title: "MCP Armor",
    description:
      "Capability enforcement for MCP tools. " +
      "Kernel-level sandboxing for every tool, one command.",
    images: [{ url: "/og.svg", width: 1200, height: 630 }],
  },
  twitter: { card: "summary_large_image" },
  metadataBase: new URL("https://mcp-armor.com"),
};

/** Root layout: fonts, nav, and footer wrapping all pages. */
export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body
        className={`${dmSerifDisplay.variable} ${inter.variable} ${jetbrainsMono.variable} antialiased`}
        style={{
          fontFamily: "var(--font-body)",
          color: "var(--color-text-primary)",
          backgroundColor: "var(--color-bg)",
        }}
      >
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-[100] focus:px-4 focus:py-2 focus:rounded-md focus:text-sm focus:font-medium focus:text-white"
          style={{ backgroundColor: "var(--color-accent)" }}
        >
          Skip to main content
        </a>
        <Navbar />
        <main id="main-content">{children}</main>
        <Footer />
      </body>
    </html>
  );
}
