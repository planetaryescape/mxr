import { createFileRoute } from "@tanstack/react-router";

import { PaperTrailRoute } from "@/features/places/PaperTrailRoute";

/** Updates, an early version on Paper trail (blueprint 22). */
export const Route = createFileRoute("/updates")({
  component: PaperTrailRoute,
});
