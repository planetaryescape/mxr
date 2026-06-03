import { createFileRoute } from "@tanstack/react-router";

import { JobsRoute } from "@/features/jobs/JobsRoute";

export const Route = createFileRoute("/jobs")({
  component: JobsRoute,
});
