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
  | "mode-done"
  | "digest-let-go"
  | "pin"
  | "move-sender"
  | "todo-done"
  | "todo-dismiss"
  | "todo-schedule"
  | "todo-edit"
  | "todo-create"
  | "todo-keep"
  | "todo-let-go"
  | "record-file"
  | "record-dismiss"
  | "record-fix"
  | "record-check"
  | "reading-later";

export type Verb = MailAction | OtherVerb;

/**
 * Toast colours by type (sonner's rich colours over the theme tokens):
 * success for done, info for a neutral change you can undo (archive, move,
 * a handoff), warning for a partial failure, error for a failure.
 */
export type ToastTone = "success" | "info" | "warning" | "error";

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
  /** Doing it again reverses it (pin). */
  | "toggle"
  /** Can't be undone by mxr; the entry says why, and the verb confirms first. */
  | "none";

interface VerbParts {
  /** Registry actions that run it; each is a key, and a button where one shows. */
  actions: readonly string[];
  /** Other ways in, in words (buttons, swipes, dialogs). */
  alsoFrom?: string;
  /** What changes on screen before the daemon answers. */
  optimistic: string;
  /** Past tense for the result toast: "Archived 3 messages". */
  pastTense: string;
  sound: SoundEvent | null;
  /** The toast's colour: done is success, a change with undo is info. */
  tone: ToastTone;
  /** When it asks before acting. */
  confirm?: string;
}

/**
 * A verb either has a way back, or is marked irreversible with the reason
 * and always confirms first. `verbs.spec` runs the undo journey for the
 * first kind and the confirm journey for the second.
 */
export type VerbFeedback =
  | (VerbParts & { undo: Exclude<UndoPath, "none">; irreversible?: never })
  | (VerbParts & { undo: "none"; irreversible: true; reason: string; confirm: string });

const SWIPE = "swipe on a touch screen";

export const VERB_FEEDBACK: Record<Verb, VerbFeedback> = {
  archive: {
    actions: ["mail.archive", "reader.archive-next", "reader.archive-prev"],
    alsoFrom: `row and bulk buttons, reader toolbar, ${SWIPE} (short right)`,
    optimistic: "The rows leave the list at once; the reader moves to the next conversation.",
    pastTense: "Archived",
    tone: "info",
    sound: "archived",
    undo: "daemon-mutation",
    confirm: "More than 20 messages at once.",
  },
  "read-and-archive": {
    actions: ["mail.read-archive"],
    alsoFrom: "bulk bar",
    optimistic: "The rows leave the list at once.",
    pastTense: "Read and archived",
    tone: "info",
    sound: "archived",
    undo: "daemon-mutation",
    confirm: "More than 20 messages at once.",
  },
  trash: {
    actions: ["mail.trash"],
    alsoFrom: `row and bulk buttons, ${SWIPE} (long right)`,
    optimistic: "The rows leave the list at once.",
    pastTense: "Moved to Trash",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
    confirm: "More than one conversation, or more than 20 messages.",
  },
  spam: {
    actions: ["mail.spam"],
    alsoFrom: "bulk bar",
    optimistic: "The rows leave the list at once.",
    pastTense: "Marked as spam",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
    confirm: "More than one conversation, or more than 20 messages.",
  },
  star: {
    actions: ["mail.star"],
    alsoFrom: "row button, reader header",
    optimistic: "The star fills at once.",
    pastTense: "Starred",
    tone: "info",
    sound: null,
    // The daemon restores each message's own prior star, so starring a
    // conversation with one starred message leaves that one starred.
    undo: "daemon-mutation",
  },
  unstar: {
    actions: ["mail.star"],
    alsoFrom: "row button, reader header",
    // A conversation's unstar clears every star in it, including messages
    // the list doesn't show, so the row and the action agree.
    optimistic: "The star empties at once.",
    pastTense: "Unstarred",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  read: {
    actions: ["mail.mark-read"],
    alsoFrom: "row button, bulk bar",
    optimistic: "The row loses its unread mark at once.",
    pastTense: "Marked read",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  unread: {
    actions: ["mail.mark-unread"],
    alsoFrom: "row button, bulk bar",
    optimistic: "The row gains its unread mark at once.",
    pastTense: "Marked unread",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  move: {
    actions: ["mail.move"],
    alsoFrom: "the move dialog",
    optimistic: "The rows leave a list they no longer belong to.",
    pastTense: "Moved",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  route: {
    actions: ["mail.route"],
    alsoFrom: "the move dialog from a queue",
    optimistic: "The rows leave the queue at once.",
    pastTense: "Routed",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  "label-add": {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "The label chip shows at once.",
    pastTense: "Labelled",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  "label-remove": {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "The chip goes; the rows leave that label's list.",
    // With the label: "Removed Hiring from 3 messages".
    pastTense: "Removed",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  labels: {
    actions: ["mail.label"],
    alsoFrom: "the labels dialog",
    optimistic: "Chips change at once.",
    pastTense: "Updated labels on",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  snooze: {
    actions: ["mail.snooze", "focus.snooze"],
    alsoFrom: `row and bulk buttons, reader toolbar, ${SWIPE} (left)`,
    optimistic: "The rows leave the list once a time is chosen.",
    pastTense: "Snoozed",
    tone: "info",
    sound: "snoozed",
    undo: "wake",
  },
  "reply-later": {
    actions: ["mail.reply-later"],
    alsoFrom: "Enter with no time in its dialog",
    optimistic: "Nothing moves; the conversation joins the reply queue.",
    pastTense: "Added to your reply queue",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
  "reply-later-at": {
    actions: ["mail.reply-later"],
    alsoFrom: 'a time typed in its dialog ("tue 9", "in 3d")',
    optimistic:
      "The conversation leaves the desk and the reply queue until then; on Waiting on it comes back only if nobody replied.",
    pastTense: "Reply later: back",
    tone: "info",
    sound: null,
    undo: "daemon-mutation",
  },
  send: {
    actions: ["focus.send", "focus.remind"],
    alsoFrom: "the Send button, ⌘↵ in the composer",
    optimistic: "The composer closes and a countdown toast holds the send.",
    pastTense: "Message sent",
    tone: "success",
    sound: "sent",
    undo: "send-window",
  },
  unsubscribe: {
    actions: ["mail.unsubscribe", "place.unsubscribe"],
    alsoFrom: "the reader's unsubscribe line",
    optimistic: "Nothing moves until the dialog is confirmed.",
    pastTense: "Unsubscribed from",
    tone: "success",
    sound: null,
    undo: "none",
    irreversible: true,
    reason:
      "The sender acts on the request (a one-click link, an email or their page), and mxr can't take it back. Unsubscribe and archive can undo the archive part.",
    confirm: "Always: the dialog names the sender and how they are asked.",
  },
  sweep: {
    actions: ["place.sweep-bundle", "place.sweep-all"],
    alsoFrom: `the Sweep buttons, ${SWIPE} on a Paper trail sender`,
    optimistic: "The daemon's dry run shows the count first; then the swept mail leaves.",
    pastTense: "Archived",
    tone: "info",
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
    tone: "success",
    sound: "archived",
    undo: "daemon-mutation",
  },
  "mode-done": {
    // Done here, per mode (features/modes/modeDone): other modes keep the
    // thread, and the provider archive happens only when none does.
    actions: ["now.done", "place.done", "messages.done"],
    alsoFrom: "the check on a Now row, Done here on a Messages person page",
    optimistic: "The row leaves this mode at once; the toast says where it still is.",
    pastTense: "Done",
    tone: "success",
    sound: "archived",
    undo: "daemon-mutation",
  },
  "digest-let-go": {
    actions: ["now.let-go-digest"],
    alsoFrom: "the Let go button on Now's Updates card",
    optimistic: "The card leaves Now once you confirm the preview.",
    pastTense: "Let go",
    tone: "success",
    sound: "archived",
    undo: "daemon-mutation",
    confirm: "Previews the daemon's dry run: how many updates and which stay in To do",
  },
  pin: {
    actions: ["place.pin"],
    alsoFrom: "the pin button on a Paper trail or Reading message",
    optimistic: "The pin fills at once.",
    pastTense: "Pinned",
    tone: "info",
    sound: null,
    undo: "toggle",
  },
  "todo-done": {
    // From an email it is done in To do (`SetModeDone`): the daemon's toast
    // says whether the email was archived, and its undo puts both back.
    actions: ["todo.done"],
    alsoFrom: "the check on a To do row, e on a Now row under Due soon",
    optimistic: "The row folds up and joins Done this week.",
    pastTense: "Ticked off",
    tone: "success",
    sound: "archived",
    undo: "daemon-mutation",
  },
  "todo-dismiss": {
    actions: ["todo.dismiss"],
    optimistic: "The row leaves To do and never comes back for that email.",
    pastTense: "Not a to-do",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
  "todo-schedule": {
    actions: ["todo.schedule"],
    optimistic: "Nothing moves until a time is chosen; then the row moves to that day.",
    pastTense: "Scheduled",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
  "todo-edit": {
    actions: ["todo.edit"],
    optimistic: "Nothing moves until the change is saved; then the row is yours.",
    pastTense: "Changed",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
  "todo-create": {
    actions: ["mail.make-todo"],
    optimistic: "Nothing moves until the title is saved; the to-do joins To do.",
    pastTense: "Added to To do",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "reading-later": {
    actions: ["reading.later"],
    optimistic: "The item shows as saved and stays on the Later shelf until you take it off.",
    pastTense: "Saved to Later",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "todo-keep": {
    actions: ["todo.catchup-keep"],
    optimistic: "The row leaves the catch-up and joins the runway.",
    pastTense: "Kept",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "todo-let-go": {
    actions: ["todo.catchup-let-go", "todo.catchup-let-go-all"],
    alsoFrom: "the Let go of all button, which previews first",
    optimistic: "The rows leave the catch-up and join the Expired list.",
    pastTense: "Let go of",
    tone: "info",
    sound: null,
    undo: "reverse-request",
    confirm: "Let go of all: it previews the daemon's dry run.",
  },
  "record-file": {
    actions: ["mail.pass-to-mode"],
    alsoFrom: "the palette's Pass to a mode",
    optimistic: "Nothing moves until the preview card is confirmed; then the record joins Archive.",
    pastTense: "Filed in Archive",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "record-dismiss": {
    actions: ["archive.dismiss"],
    optimistic: "The row leaves Archive; the email is untouched and never filed again.",
    pastTense: "Not a record",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
  "record-check": {
    actions: ["archive.check"],
    optimistic: "Every unchecked amount and date becomes yours, and the card says checked.",
    pastTense: "Marked checked",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "record-fix": {
    actions: ["archive.edit"],
    optimistic: "Nothing moves until the fix is saved; then the field is yours and checked.",
    pastTense: "Fixed",
    tone: "success",
    sound: null,
    undo: "reverse-request",
  },
  "move-sender": {
    actions: ["place.move-sender", "reader.move-sender"],
    alsoFrom: "the line under a message in Reading or Paper trail",
    optimistic: "The sender's mail leaves this place at once.",
    pastTense: "Moved",
    tone: "info",
    sound: null,
    undo: "reverse-request",
  },
};

/** The toast colour a verb's result shows in. */
export function toneFor(verb: Verb): ToastTone {
  return VERB_FEEDBACK[verb].tone;
}

/** The sound a mail action makes, once per action. */
export function soundFor(verb: Verb): SoundEvent | null {
  return VERB_FEEDBACK[verb].sound;
}
