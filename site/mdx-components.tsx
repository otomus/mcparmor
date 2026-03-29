import type { MDXComponents } from "mdx/types";

/** Custom MDX component overrides for docs pages. */
export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...components,
  };
}
