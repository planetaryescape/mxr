import { createFileRoute } from "@tanstack/react-router";

import { DiagnosticsRoute } from "@/features/diagnostics/DiagnosticsRoute";
import { optionalEnum } from "@/lib/searchParams";

const PANELS = ["overview", "logs", "events", "activity"] as const;

export const Route = createFileRoute("/diagnostics")({
  // `g L` lands on /diagnostics?panel=logs; unknown values fall back to overview.
  validateSearch: (search: Record<string, unknown>): { panel?: (typeof PANELS)[number] } => {
    const panel = optionalEnum(search.panel, PANELS);
    return panel ? { panel } : {};
  },
  component: DiagnosticsRoute,
});
