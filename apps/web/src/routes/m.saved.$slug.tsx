import { createFileRoute } from "@tanstack/react-router";

import { MailView } from "@/features/mailbox/MailView";

export const Route = createFileRoute("/m/saved/$slug")({
  component: SavedSearchMailbox,
});

function SavedSearchMailbox() {
  const { slug } = Route.useParams();
  return <MailView route={{ kind: "saved", slug }} />;
}
