import { createFileRoute } from "@tanstack/react-router";

import { OwedRoute } from "@/features/owed/OwedRoute";

export const Route = createFileRoute("/owed")({
  component: OwedRoute,
});
