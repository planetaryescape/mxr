import { createFileRoute } from "@tanstack/react-router";

import { ArchiveRoute } from "@/features/archive/ArchiveRoute";
import { parseArchiveSearch, type ArchiveSearch } from "@/features/archive/archiveSearch";

/** Archive the mode (records), not All Mail (`/m/archive`). */
export const Route = createFileRoute("/archive")({
  validateSearch: (search: Record<string, unknown>): ArchiveSearch => parseArchiveSearch(search),
  component: ArchiveRoute,
});
