import { createFileRoute } from "@tanstack/react-router";

import { ScreenerRoute } from "@/features/screener/ScreenerRoute";

export const Route = createFileRoute("/screener")({
  component: ScreenerRoute,
});
