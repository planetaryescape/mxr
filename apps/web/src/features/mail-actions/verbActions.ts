/*
 * Mail verbs on the keyboard. Keys match the TUI's live mail-action handler
 * (crates/tui/src/app/input.rs), bound in both the list and the reader;
 * each view's controller decides what "the target" is.
 */

import {
  Archive,
  ArrowRightCircle,
  CalendarCheck,
  CalendarX,
  Clock,
  Download,
  FileText,
  Forward,
  Link as LinkIcon,
  Mail,
  MailCheck,
  MailOpen,
  MailX,
  Paperclip,
  Reply,
  ReplyAll,
  ShieldAlert,
  Star,
  Tag,
  Trash2,
  UserRound,
  UserSearch,
  CornerDownRight,
} from "lucide-react";

import type { Action, ActionScope } from "@/lib/actions/types";

const BOTH: ActionScope[] = ["list", "reader"];

function verb(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<Action, "id" | "command" | "run">> = {},
): Action {
  return { id, command, label, shortcut, group: "Mail", scopes: BOTH, ...extra } as Action;
}

export const mailVerbActions: Action[] = [
  verb("mail.archive", "archive", "Archive", "e", { icon: Archive }),
  verb("mail.read-archive", "readArchive", "Mark read and archive", "m", {
    icon: MailCheck,
    tuiNote: "Same as the TUI; read/unread are I and U",
  }),
  verb("mail.trash", "trash", "Move to Trash", "#", {
    icon: Trash2,
    aliases: ["Delete", "Backspace"],
  }),
  verb("mail.spam", "spam", "Mark as spam", "!", { icon: ShieldAlert }),
  verb("mail.star", "toggleStar", "Star or unstar", "s", { icon: Star }),
  verb("mail.mark-read", "markRead", "Mark read", "I", { icon: MailOpen }),
  verb("mail.mark-unread", "markUnread", "Mark unread", "U", { icon: Mail }),
  verb("mail.label", "label", "Labels…", "l", { icon: Tag }),
  verb("mail.move", "move", "Move to label…", "v", { icon: ArrowRightCircle }),
  verb("mail.snooze", "snooze", "Snooze…", "Z", { icon: Clock }),
  verb("mail.unsubscribe", "unsubscribe", "Unsubscribe…", "D", { icon: MailX }),
  verb("mail.reply-later", "replyLater", "Reply later", "b", { icon: CornerDownRight }),
  verb("mail.reply", "reply", "Reply", "r", { icon: Reply, group: "Compose" }),
  verb("mail.reply-all", "replyAll", "Reply all", "a", { icon: ReplyAll, group: "Compose" }),
  verb("mail.forward", "forward", "Forward", "f", { icon: Forward, group: "Compose" }),
  verb("mail.export", "exportThread", "Export as Markdown", "E", { icon: Download }),
  verb("mail.briefing", "briefing", "Thread briefing", "B", { icon: FileText }),
  verb("mail.whois", "whois", "Who is this sender?", "W", { icon: UserSearch }),
  verb("mail.sender-profile", "senderProfile", "Sender profile", "p", {
    icon: UserRound,
    tuiNote: "Palette only in the TUI",
  }),
  verb("mail.links", "links", "Links in this message", "L", { icon: LinkIcon }),
  verb("mail.attachments", "attachments", "Attachments", "A", { icon: Paperclip }),
  verb("invite.accept", "inviteAccept", "Accept invite", "i a", {
    icon: CalendarCheck,
    group: "Triage",
  }),
  verb("invite.tentative", "inviteTentative", "Maybe (invite)", "i m", { group: "Triage" }),
  verb("invite.decline", "inviteDecline", "Decline invite", "i d", {
    icon: CalendarX,
    group: "Triage",
  }),
  verb("invite.accept-comment", "inviteAcceptComment", "Accept with comment", "i A", {
    group: "Triage",
  }),
  verb("invite.tentative-comment", "inviteTentativeComment", "Maybe with comment", "i M", {
    group: "Triage",
  }),
  verb("invite.decline-comment", "inviteDeclineComment", "Decline with comment", "i D", {
    group: "Triage",
  }),
  {
    id: "mail.cancel-reminder",
    command: "cancelReminder",
    label: "Cancel no-reply reminder",
    description: "Stop the follow-up reminder set when this message was sent",
    group: "Mail",
    scopes: BOTH,
    paletteOnly: true,
  },
  {
    id: "mail.compose-to-sender",
    command: "composeToSender",
    label: "Write to sender",
    group: "Compose",
    scopes: BOTH,
    paletteOnly: true,
  },
];
