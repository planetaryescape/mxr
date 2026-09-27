import { createFileRoute } from "@tanstack/react-router";

import { ReadingRoute } from "@/features/places/ReadingRoute";

export const Route = createFileRoute("/reading")({
  component: ReadingRoute,
});
