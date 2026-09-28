import type { ReactNode } from "react";

import { LowTide } from "@/features/low-tide/LowTide";

/** A place or queue you cleared: the small low tide, once per clearing. */
export function SweptClear({
  place,
  line = `${place} is clear.`,
  children = "Low tide. Whatever arrives next gathers here.",
}: {
  place: string;
  line?: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
      <LowTide size="small" line={line} className="w-full">
        {children}
      </LowTide>
    </div>
  );
}
