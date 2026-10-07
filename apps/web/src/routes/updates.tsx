import { createFileRoute } from "@tanstack/react-router";

import { UpdatesRoute } from "@/features/updates/UpdatesRoute";

/** Updates: notifications as a twice-daily briefing by source (blueprint 22). */
export const Route = createFileRoute("/updates")({
  component: UpdatesRoute,
});
