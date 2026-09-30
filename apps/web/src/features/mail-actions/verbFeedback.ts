/*
 * Every verb's four parts (Saffer): what triggers it, what changes on
 * screen at once, what the toast says, the sound it makes and how it is
 * undone. One table, so a new verb can't ship without deciding each part,
 * and so the words and sounds live in one place: `actionPastTense` reads
 * the past tense from here and the sound player reads the sound.
 *
 * Undo replaces confirmation wherever the daemon can undo. The confirm
 * column says when a verb still asks first.
 */

import type { SoundEvent } from "@/features/sound/player";

import type { MailAction } from "./pendingMailOps";

/** Verbs that aren't a `MailAction` (they have their own request). */
export type OtherVerb =
  | "reply-later"
  | "reply-later-at"
  | "send"
  | "unsubscribe"
  | "sweep"
  | "desk-done"
  | "pin"
  | "move-sender";

export type Verb = MailAction | OtherVerb;

export type UndoPath =
  /** The daemon's undo for this mutation id (`u`, `z`, or the toast). */
  | "daemon-mutation"
  /** Wake the snoozed messages. */
  | "wake"
  /** The daemon's undo for every chunk of a background job. */
  | "daemon-job"
  /** A request that reverses it (reply later off, restore to Waiting). */
  | "reverse-request"
  /** The undo-send window: nothing leaves until it closes. */
  | "send-window"
  /** Doing it again reverses it (star, read, pin). */
  | "toggle"
  /** Can't be undone by mxr; the dialog previews it first. */
  | "none";

export interface VerbFeedback {
  /** Registry actions that run it; each is a key, and a button where one shows. */
  actions: readonly string[];
  /** Other ways in, in words (buttons, swipes, dialogs). */
  alsoFrom?: string;
  /** What changes on screen before the daemon answers. */
  optimistic: string;
  /** Past tense for the result toast: "Archived 3 messages". */
  pastTense: string;
  sound: SoundEvent | null;
  undo: UndoPath;
  /** When it asks before acting. */
  confirm?: string;
}

const SWIPE = "swipe on a touch screen";

export const VERB_FEEDBACK: Record<Verb, VerbFeedback> = {
  archive: {
    actions: ["mail.archive", "reader.archive-next", "reader.archive-prev"],
    alsoFrom: `row and bulk buttons, reader toolbar, ${SWIPE} (short right)`,
    optimistic: "The rows leave the list at once; the reader moves to the next conversation.",
    pastTense: "Archived",
    sound: "archived",
    undo: "daemon-mutation",
    confirm: "More than 20 messages at once.",
  },
  "read-and-archive": {
    actions: ["mail.read-archive"],
    alsoFrom: "bulk bar",
    optimistic: "The rows leave the list at once.",
    pastTense: "Read and archived",
    sound: "archived",
    undo: "daemon-mutation",
    confirm: "More than 20 messages at once.",
  },
  trash: {
    actions: ["mail.trash"],
    alsoFrom: `row and bulk buttons, ${SWIPE} (long right)`,
    optimistic: "The rows leave the list at once.",
    pastTense: "Moved to Trash",
    sound: null,
    undo: "daemon-mutation",
    confirm: "More than one conversation, or more than 20 messages.",
  },
  spam: {
    actions: ["mail.spam"],
    alsoFrom: "bulk bar",
    optimistic: "The rows leave the list at once.",
    pastTense: "Marked as spam",
    sound: null,
    undo: "daemon-mutation",
    confirm: "More than one conversation, or more than 20 messages.",
  },
  star: {
    actions: ["mail.star"],
    alsoFrom: "row button, reader header",
    optimistic: "The star fills at once.",
    pastTense: "Starred",
    sound: null,
    undo: "daemon-mutation",
  },
  unstar: {
    actions: ["mail.star"],
    alsoFrom: "row button, reader header",
    optimistic: "The star empties at once.",
    pastTense: "Unstarred",
    sound: null,
    undo: "daemon-mutation",
  },
  read: {
    actions: ["mail.mark-read"],
    alsoFrom: "row button, bulk bar",
    optimistic: "The row loses its unread mark at once.",
    pastTense: "Marked read",
    sound: null,
    undo: "daemon-mutation",
  },
  unread: {
    actions: ["mail.mark-unread"],
    alsoFrom: "row button, bulk bar",
    optimistic: "The row gains its unread mark at once.",
    pastTense: "Marked unread",
    sound: null,
    undo: "daemon-mutation",
  },
  move: {
    actions: ["mail.move"],
    alsoFrom: "the move dialog",
    optimistic: "The rows leave a list they no longer belong to.",
    pastTense: "Moved",
    sound: null,
    undo: "daemon-mutation",
  },
  route: {
    actions: ["mail.route"],
    alsoFrom: "the move dialog from a queue",
    optimistic: "The rows leave the queue at once.",
    pastTense: "Routed",
    sound: null,
    undo: "daemon-mutation",
  },
  "label-add": {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "The label chip shows at once.",
    pastTense: "Labelled",
    sound: null,
    undo: "daemon-mutation",
  },
  "label-remove": {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "The chip goes; the rows leave that label's list.",
    pastTense: "Label removed",
    sound: null,
    undo: "daemon-mutation",
  },
  labels: {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "Chips change at once.",
    pastTense: "Updated labels on",
    sound: null,
    undo: "daemon-mutation",
  },
  snooze: {
    actions: ["mail.snooze", "focus.snooze"],
    alsoFrom: `row and bulk buttons, reader toolbar, ${SWIPE} (left)`,
    optimistic: "The rows leave the list once a time is chosen.",
    pastTense: "Snoozed",
    sound: "snoozed",
    undo: "wake",
  },
  "reply-later": {
    actions: ["mail.reply-later"],
    alsoFrom: "Enter with no time in its dialog",
    optimistic: "Nothing moves; the conversation joins the reply queue.",
    pastTense: "Added to your reply queue",
    sound: null,
    undo: "reverse-request",
  },
  "reply-later-at": {
    actions: ["mail.reply-later"],
    alsoFrom: 'a time typed in its dialog ("tue 9", "in 3d")',
    optimistic:
      "The conversation leaves the desk and the reply queue until then; on Waiting on it comes back only if nobody replied.",
    pastTense: "Reply later: back",
    sound: null,
    undo: "daemon-mutation",
  },
  send: {
    actions: ["focus.send", "focus.remind"],
    alsoFrom: "the Send button, ⌘↵ in the composer",
    optimistic: "The composer closes and a countdown toast holds the send.",
    pastTense: "Sent",
    sound: "sent",
    undo: "send-window",
  },
  unsubscribe: {
    actions: ["mail.unsubscribe", "place.unsubscribe"],
    alsoFrom: "the reader's unsubscribe line",
    optimistic: "Nothing moves until the dialog is confirmed.",
    pastTense: "Unsubscribed from",
    sound: null,
    undo: "none",
    confirm: "Always: the dialog names the sender and how they are asked.",
  },
  sweep: {
    actions: ["place.sweep-bundle", "place.sweep-all"],
    alsoFrom: `the Sweep buttons, ${SWIPE} on a Paper trail sender`,
    optimistic: "The daemon's dry run shows the count first; then the swept mail leaves.",
    pastTense: "Archived",
    sound: "archived",
    undo: "daemon-job",
    confirm: "Always: it previews the daemon's dry run.",
  },
  "desk-done": {
    // On the desk, archive is Done in every lane (see features/desk/deskDone).
    actions: ["list.row-action", "mail.archive", "mail.read-archive", "focus.done"],
    alsoFrom: `the check on every desk row, ${SWIPE} (short right)`,
    optimistic: "The row leaves the desk at once; focus mode moves to the next conversation.",
    pastTense: "Done",
    sound: "archived",
    undo: "daemon-mutation",
  },
  pin: {
    actions: ["place.pin"],
    alsoFrom: "the pin button on a Paper trail or Reading message",
    optimistic: "The pin fills at once.",
    pastTense: "Pinned",
    sound: null,
    undo: "toggle",
  },
  "move-sender": {
    actions: ["place.move-sender", "reader.move-sender"],
    alsoFrom: "the line under a message in Reading or Paper trail",
    optimistic: "The sender's mail leaves this place at once.",
    pastTense: "Moved",
    sound: null,
    undo: "reverse-request",
  },
};

/** The sound a mail action makes, once per action. */
export function soundFor(verb: Verb): SoundEvent | null {
  return VERB_FEEDBACK[verb].sound;
}
