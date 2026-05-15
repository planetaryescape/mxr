import { createFileRoute } from "@tanstack/react-router";

import { ActivityRoute } from "@/features/activity/ActivityRoute";

export const Route = createFileRoute("/activity")({
  component: ActivityRoute,
});
