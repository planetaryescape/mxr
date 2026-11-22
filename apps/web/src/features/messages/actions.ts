import {
  Check,
  Clock,
  Filter,
  MessageSquarePlus,
  Pin,
  Reply,
  ReplyAll,
  ThumbsUp,
} from "lucide-react";

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
  { id: "messages.filter", command: "filter", icon: Filter },
  { id: "messages.cycle-filter", command: "cycleFilter", icon: Filter },
  { id: "messages.send-next", command: "sendNext" },
  { id: "messages.escape", command: "escape" },
  ...newSenderActions("messages"),
];
