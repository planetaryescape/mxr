import { createFileRoute } from "@tanstack/react-router";

import { ArchiveRoute } from "@/features/archive/ArchiveRoute";
import { parseArchiveSearch, type ArchiveSearch } from "@/features/archive/archiveSearch";

export const Route = createFileRoute("/archive")({
  validateSearch: (search: Record<string, unknown>): ArchiveSearch => parseArchiveSearch(search),
  component: ArchiveRoute,
});
