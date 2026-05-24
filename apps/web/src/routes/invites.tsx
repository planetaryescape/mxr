import { createFileRoute } from "@tanstack/react-router";

import { InvitesRoute } from "@/features/invites/InvitesRoute";

export const Route = createFileRoute("/invites")({
  component: InvitesRoute,
});
