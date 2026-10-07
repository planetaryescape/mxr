import { createFileRoute } from "@tanstack/react-router";

import { ReadingReader } from "@/features/reading/ReadingReader";

export const Route = createFileRoute("/reading/item/$itemKey")({
  component: OpenItem,
});

function OpenItem() {
  const { itemKey } = Route.useParams();
  return <ReadingReader itemKey={itemKey} />;
}
