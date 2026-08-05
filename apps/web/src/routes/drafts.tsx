import { createFileRoute } from "@tanstack/react-router";

import { DraftsRoute } from "@/features/drafts/DraftsRoute";

export const Route = createFileRoute("/drafts")({
  component: DraftsRoute,
});
