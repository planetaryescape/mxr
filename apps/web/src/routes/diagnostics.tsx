import { createFileRoute } from "@tanstack/react-router";

import { DiagnosticsRoute } from "@/features/diagnostics/DiagnosticsRoute";

export const Route = createFileRoute("/diagnostics")({
  component: DiagnosticsRoute,
});
