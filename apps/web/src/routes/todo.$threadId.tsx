import { createFileRoute } from "@tanstack/react-router";

import { ThreadPane } from "@/features/thread/ThreadPane";

/** Opened from a row: the link it is about, to mark in its email. */
export interface TodoThreadSearch {
  link?: string;
  mid?: string;
  title?: string;
  domain?: string;
  /** The row's account: a thread id can be held by two accounts. */
  account?: string;
}

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

export const Route = createFileRoute("/todo/$threadId")({
  validateSearch: (search: Record<string, unknown>): TodoThreadSearch => ({
    link: text(search.link),
    mid: text(search.mid),
    title: text(search.title),
    domain: text(search.domain),
    account: text(search.account),
  }),
  component: OpenThread,
});

function OpenThread() {
  const { threadId } = Route.useParams();
  const { link, mid, title, domain } = Route.useSearch();
  return (
    <ThreadPane
      threadId={threadId}
      focusMessageId={mid}
      linkHighlight={link ? { url: link, messageId: mid, title: title ?? "", domain } : undefined}
    />
  );
}
