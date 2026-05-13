import { createFileRoute } from "@tanstack/react-router";

import { MailboxRoute } from "@/features/mailbox/MailboxRoute";

export const Route = createFileRoute("/m/label/$name")({
  component: MailboxRoute,
});
