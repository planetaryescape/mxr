import { Check, Forward, Reply, ReplyAll } from "lucide-react";

import { newSenderActions } from "@/features/modes/newSenderActions";
import type { ActionBehavior } from "@/lib/actions/types";

export const nowActions: ActionBehavior[] = [
  { id: "now.down", command: "down" },
  { id: "now.up", command: "up" },
  { id: "now.open", command: "open" },
  { id: "now.done", command: "done", icon: Check },
  { id: "now.reply", command: "reply", icon: Reply },
  { id: "now.reply-all", command: "replyAll", icon: ReplyAll },
  { id: "now.forward", command: "forward", icon: Forward },
  { id: "now.to-do", command: "makeTodo" },
  { id: "now.open-email", command: "openEmail" },
  { id: "now.let-go-digest", command: "letGoDigest" },
  { id: "now.close-hint", command: "closeHint" },
  ...newSenderActions("now"),
];
