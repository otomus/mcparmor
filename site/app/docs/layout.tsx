import type { ReactNode } from "react";
import { DocsSidebar } from "@/components/docs/DocsSidebar";

/** Docs layout: sidebar + centered content column. */
export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="pt-16 flex">
      <DocsSidebar />
      <div className="flex-1 min-w-0 px-4 py-12 lg:pl-8">
        <article className="max-w-3xl mx-auto prose prose-neutral">
          {children}
        </article>
      </div>
    </div>
  );
}
