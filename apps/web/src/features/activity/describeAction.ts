/*
 * The Activity table shows what happened in words. The raw action code
 * ("mail.archive") stays in the cell's title and in the prefix filter, so
 * nothing is lost; it just isn't the first thing a person has to decode.
 */

const WORDS: Record<string, string> = {
  "mail.archive": "Archived",
  "mail.trash": "Moved to Trash",
  "mail.star": "Starred",
  "mail.unstar": "Unstarred",
  "mail.read": "Marked read",
  "mail.unread": "Marked unread",
  "mail.snooze": "Snoozed",
  "mail.unsnooze": "Woke a snoozed conversation",
  "mail.label": "Added a label",
  "mail.unlabel": "Removed a label",
  "mail.move": "Moved",
  "mail.mark_spam": "Marked as spam",
  "mail.send": "Sent",
  "mail.reply": "Replied",
  "mail.forward": "Forwarded",
  "mail.route": "Routed a sender",
  "mail.unsubscribe": "Unsubscribed",
  "mail.unsubscribe_purge": "Unsubscribed and cleared old mail",
  "mail.calendar_rsvp": "Answered an invite",
  "draft.create": "Started a draft",
  "draft.save": "Saved a draft",
  "draft.update": "Edited a draft",
  "draft.discard": "Discarded a draft",
  "account.add": "Added an account",
  "account.remove": "Removed an account",
  "account.rename": "Renamed an account",
  "account.signin": "Signed in to an account",
  "account.sync": "Synced an account",
  "app.start": "mxr started",
  "app.stop": "mxr stopped",
  "activity.paused": "Paused recording",
  "activity.resumed": "Resumed recording",
  "activity.exported": "Exported activity",
  "activity.pruned": "Pruned old activity",
  "activity.redacted": "Redacted entries",
  "desk.done": "Finished here",
  "desk.later": "Put off until later",
  "message.pin": "Pinned a message",
  "message.unpin": "Unpinned a message",
  "messages.got_it": "Marked a conversation as handled",
};

/** An action code as a short sentence; codes without a known wording become one mechanically. */
export function describeAction(action: string): string {
  const known = WORDS[action];
  if (known) return known;
  const plain = action.replace(/[._]+/g, " ").trim();
  return plain ? plain.charAt(0).toUpperCase() + plain.slice(1) : action;
}

/**
 * Whether recording is paused, from the newest pause or resume marker (they
 * are written even while paused, so the log always knows). A pause with an
 * end time that has passed is over.
 */
export function isRecordingPaused(
  marker: { action: string; context: Record<string, unknown> | null } | undefined,
  now: number,
): boolean {
  if (marker?.action !== "activity.paused") return false;
  const until = marker.context?.until;
  return typeof until === "number" ? until > now : true;
}
