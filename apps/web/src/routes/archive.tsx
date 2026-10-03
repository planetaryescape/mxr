import { createFileRoute } from "@tanstack/react-router";

import { ArchiveModeRoute } from "@/features/modes/ArchiveModeRoute";

/** Archive the mode (records), not All Mail (`/m/archive`). */
export const Route = createFileRoute("/archive")({
  component: ArchiveModeRoute,
});
