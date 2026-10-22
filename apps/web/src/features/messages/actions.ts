/*
 * Messages' keys, from blueprint 22's key table and the daemon's Messages
 * guide. The mounted Messages page registers what each command does
 * (`useScopeController("messages", …)`). Archive, label and move are not
 * on this surface; `u` stays the global undo, which also cancels a Got it
 * still counting down.
 */

import { Check, Clock, MessageSquarePlus, Pin, Reply, ReplyAll, ThumbsUp } from "lucide-react";

import { newSenderActions } from "@/features/modes/newSenderActions";
import type { ActionBehavior } from "@/lib/actions/types";

export const messagesActions: ActionBehavior[] = [
  { id: "messages.down", command: "down" },
  { id: "messages.up", command: "up" },
  { id: "messages.open", command: "open" },
  { id: "messages.reply", command: "reply", icon: Reply },
  { id: "messages.reply-all", command: "replyAll", icon: ReplyAll },
  { id: "messages.got-it", command: "gotIt", icon: ThumbsUp },
  { id: "messages.done", command: "done", icon: Check },
  { id: "messages.to-do", command: "makeTodo" },
  { id: "messages.reply-later", command: "replyLater", icon: Clock },
  { id: "messages.pin", command: "pin", icon: Pin },
  { id: "messages.new-topic", command: "newTopic", icon: MessageSquarePlus },
  { id: "messages.prev-topic", command: "prevTopic" },
  { id: "messages.next-topic", command: "nextTopic" },
  { id: "messages.person-page", command: "personPage" },
  { id: "messages.as-sent", command: "asSent" },
  { id: "messages.send-next", command: "sendNext" },
  { id: "messages.escape", command: "escape" },
  // Last, so the status strip (first five live keys) keeps the page's own verbs.
  ...newSenderActions("messages"),
];
