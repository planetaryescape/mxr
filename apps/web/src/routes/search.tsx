import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { SearchResultsRoute } from "@/features/search/SearchResultsRoute";

const searchSchema = z.object({
  q: z.string().optional(),
  mode: z.enum(["lexical", "semantic", "hybrid"]).optional(),
  account: z.string().optional(),
  sort: z.enum(["relevance", "newest", "oldest"]).optional(),
  scope: z.enum(["threads", "messages", "attachments"]).optional(),
});

export const Route = createFileRoute("/search")({
  validateSearch: searchSchema,
  component: SearchResultsRoute,
});
