import { createFileRoute } from "@tanstack/react-router";

import { ComposeRoute } from "@/features/compose/ComposeRoute";

export const Route = createFileRoute("/compose/$draftId")({
  component: ComposeRoute,
});
