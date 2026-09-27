/* Which messages of a conversation start expanded, and where focus lands. */

import type { ThreadResponse } from "@/features/mailbox/types";

/** Newest message and every unread one start open; the rest fold. */
export function initialExpanded(data: ThreadResponse): Set<string> {
  const open = new Set(
    data.messages.filter((message) => message.unread).map((message) => message.id),
  );
  const newest = data.messages.at(-1);
  if (newest) open.add(newest.id);
  return open;
}

export function lastExpandedIndex(data: ThreadResponse, expanded: Set<string>): number {
  const firstUnread = data.messages.findIndex((message) => message.unread);
  if (firstUnread >= 0) return firstUnread;
  let index = 0;
  data.messages.forEach((message, i) => {
    if (expanded.has(message.id)) index = i;
  });
  return index;
}
