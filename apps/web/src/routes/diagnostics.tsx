import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { DiagnosticsRoute } from "@/features/diagnostics/DiagnosticsRoute";

const diagnosticsSearch = z.object({
  // `g L` lands on /diagnostics?panel=logs; unknown values fall back to overview.
  panel: z.enum(["overview", "logs", "events", "activity"]).optional().catch(undefined),
});

export const Route = createFileRoute("/diagnostics")({
  validateSearch: diagnosticsSearch,
  component: DiagnosticsRoute,
});
