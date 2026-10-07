import { createFileRoute } from "@tanstack/react-router";

import { ArchiveRoute } from "@/features/archive/ArchiveRoute";

/** Archive the mode (records), not All Mail (`/m/archive`). */
export const Route = createFileRoute("/archive")({
  component: ArchiveRoute,
});
