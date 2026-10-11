import { createFileRoute } from "@tanstack/react-router";

import { MailView } from "@/features/mailbox/MailView";

export const Route = createFileRoute("/m/category/$categoryId")({
  component: CategoryMailbox,
});

function CategoryMailbox() {
  const { categoryId } = Route.useParams();
  return <MailView route={{ kind: "category", categoryId }} />;
}
