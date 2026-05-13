import { createFileRoute } from "@tanstack/react-router";

import { MailboxRoute } from "@/features/mailbox/MailboxRoute";

export const Route = createFileRoute("/m/saved/$slug")({
  component: MailboxRoute,
});
