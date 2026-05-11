import { createFileRoute, useRouterState } from "@tanstack/react-router";

import { MailboxRoute } from "@/features/mailbox/MailboxRoute";
import { ThreadRoute } from "@/features/thread/ThreadRoute";

export const Route = createFileRoute("/m/$mailbox")({
  component: MailRoute,
});

function MailRoute() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const parts = pathname.split("/").filter(Boolean);
  return parts.length >= 3 ? <ThreadRoute /> : <MailboxRoute />;
}
