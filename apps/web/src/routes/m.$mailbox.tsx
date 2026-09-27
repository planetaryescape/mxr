import { createFileRoute } from "@tanstack/react-router";

import { MailView } from "@/features/mailbox/MailView";

export const Route = createFileRoute("/m/$mailbox")({
  component: SystemMailbox,
});

function SystemMailbox() {
  const { mailbox } = Route.useParams();
  return <MailView route={{ kind: "system", mailbox }} />;
}
