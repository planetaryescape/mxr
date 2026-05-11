import { createFileRoute } from "@tanstack/react-router";

import { AnalyticsDashboardRoute } from "@/features/analytics/AnalyticsDashboardRoute";

export const Route = createFileRoute("/analytics/$dashboard")({
  component: AnalyticsDashboardRoute,
});
