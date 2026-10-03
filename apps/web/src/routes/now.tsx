import { createFileRoute } from "@tanstack/react-router";

import { NowRoute } from "@/features/now/NowRoute";

export const Route = createFileRoute("/now")({
  component: NowRoute,
});
