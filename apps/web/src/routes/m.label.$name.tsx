import { createFileRoute } from "@tanstack/react-router";

import { MailView } from "@/features/mailbox/MailView";

export const Route = createFileRoute("/m/label/$name")({
  component: LabelMailbox,
});

function LabelMailbox() {
  const { name } = Route.useParams();
  return <MailView route={{ kind: "label", name }} />;
}
