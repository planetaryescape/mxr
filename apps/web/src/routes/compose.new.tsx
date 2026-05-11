import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { ComposeRoute } from "@/features/compose/ComposeRoute";

const composeSchema = z.object({
  reply: z.string().optional(),
  mode: z.enum(["single", "all", "forward"]).optional(),
  to: z.string().optional(),
  subject: z.string().optional(),
});

export const Route = createFileRoute("/compose/new")({
  validateSearch: composeSchema,
  component: ComposeRoute,
});
