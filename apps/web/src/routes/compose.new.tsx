import { createFileRoute } from "@tanstack/react-router";

import { ComposeRoute } from "@/features/compose/ComposeRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

export const Route = createFileRoute("/compose/new")({
  validateSearch: (search: Record<string, unknown>) => ({
    reply: optionalString(search.reply),
    mode: optionalEnum(search.mode, ["single", "all", "forward"] as const),
    to: optionalString(search.to),
    subject: optionalString(search.subject),
  }),
  component: ComposeRoute,
});
