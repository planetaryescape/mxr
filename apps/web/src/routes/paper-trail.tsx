import { createFileRoute } from "@tanstack/react-router";

import { PaperTrailRoute } from "@/features/places/PaperTrailRoute";

export const Route = createFileRoute("/paper-trail")({
  component: PaperTrailRoute,
});
