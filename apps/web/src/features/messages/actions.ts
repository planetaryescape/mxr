/*
 * Messages' keys, from blueprint 22's key table and the daemon's Messages
 * guide. The mounted Messages page registers what each command does
 * (`useScopeController("messages", …)`). Archive, label and move are not
 * on this surface; `u` stays the global undo, which also cancels a Got it
 * still counting down.
 */

import { Check, Clock, MessageSquarePlus, Pin, Reply, ReplyAll, ThumbsUp } from "lucide-react";

import type { Action, CommandAction } from "@/lib/actions/types";

function key(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: ["messages"], ...extra };
}

export const messagesActions: Action[] = [
  key("messages.down", "down", "Next person", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key("messages.up", "up", "Previous person", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  key("messages.open", "open", "Open the person", "Enter", { shortLabel: "Open", group: "Move" }),
  key("messages.reply", "reply", "Reply", "r", { shortLabel: "Reply", icon: Reply }),
  key("messages.reply-all", "replyAll", "Reply all", "a", {
    shortLabel: "Reply all",
    icon: ReplyAll,
  }),
  key("messages.got-it", "gotIt", "Got it: a short thanks, after a countdown", ".", {
    shortLabel: "Got it",
    icon: ThumbsUp,
    tuiNote: "Shows the exact text and counts down; u cancels before it sends",
  }),
  key("messages.done", "done", "Done here", "e", { shortLabel: "Done here", icon: Check }),
  key("messages.to-do", "makeTodo", "Make it a to-do…", "t", { shortLabel: "To do" }),
  key("messages.reply-later", "replyLater", "Reply later…", "b", {
    shortLabel: "Reply later",
    icon: Clock,
  }),
  key("messages.pin", "pin", "Pin or unpin this person", "s", { shortLabel: "Pin", icon: Pin }),
  key("messages.new-topic", "newTopic", "New topic with this person", "c", {
    shortLabel: "New topic",
    icon: MessageSquarePlus,
  }),
  key("messages.prev-topic", "prevTopic", "Previous topic", "[", { group: "Move" }),
  key("messages.next-topic", "nextTopic", "Next topic", "]", { group: "Move" }),
  key("messages.person-page", "personPage", "Go to the person page", "p", { group: "Move" }),
  key("messages.as-sent", "asSent", "Show the message as sent", "o", {
    shortLabel: "As sent",
    aliases: ["v"],
  }),
  key(
    "messages.send-next",
    "sendNext",
    "Send, then the next person whose turn it is",
    "Mod+Enter",
    {
      shortLabel: "Send, next",
    },
  ),
  key("messages.escape", "escape", "Close the note, or back to the people", "Escape", {
    hideInPalette: true,
  }),
];
