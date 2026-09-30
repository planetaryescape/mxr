/* Which messages of a conversation start expanded, and where focus lands. */

/** What landing needs of a conversation (a `ThreadResponse` is one). */
interface Conversation {
  messages: readonly { id: string; unread: boolean }[];
}

/** Newest message and every unread one start open; the rest fold. */
export function initialExpanded(data: Conversation): Set<string> {
  const open = new Set(
    data.messages.filter((message) => message.unread).map((message) => message.id),
  );
  const newest = data.messages.at(-1);
  if (newest) open.add(newest.id);
  return open;
}

/**
 * Where the reader opens: on `focusMessageId` when the conversation has it
 * (expanded, and `cited`), else the default landing.
 */
export function initialLanding(
  data: Conversation,
  focusMessageId?: string,
): { expanded: Set<string>; focusIndex: number; cited: boolean } {
  const expanded = initialExpanded(data);
  const cited = data.messages.findIndex((message) => message.id === focusMessageId);
  const citedMessage = data.messages[cited];
  if (citedMessage) {
    expanded.add(citedMessage.id);
    return { expanded, focusIndex: cited, cited: true };
  }
  return {
    expanded,
    focusIndex: Math.max(0, lastExpandedIndex(data, expanded)),
    cited: false,
  };
}

export function lastExpandedIndex(data: Conversation, expanded: Set<string>): number {
  const firstUnread = data.messages.findIndex((message) => message.unread);
  if (firstUnread >= 0) return firstUnread;
  let index = 0;
  data.messages.forEach((message, i) => {
    if (expanded.has(message.id)) index = i;
  });
  return index;
}
