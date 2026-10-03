import { createFileRoute } from "@tanstack/react-router";

import { FindRoute } from "@/features/modes/FindRoute";

export const Route = createFileRoute("/find")({
  component: FindRoute,
});
