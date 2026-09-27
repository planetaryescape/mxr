import { createFileRoute } from "@tanstack/react-router";

import { SnoozedRoute } from "@/features/snoozed/SnoozedRoute";

export const Route = createFileRoute("/snoozed")({
  component: SnoozedRoute,
});
