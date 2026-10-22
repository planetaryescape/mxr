/*
 * Now's keys, from blueprint 22's key table and the daemon's Now guide.
 * The mounted Now page registers what each command does
 * (`useScopeController("now", …)`). Acting on a row does it in that row's
 * own mode: `e` is done here, through `SetModeDone`.
 */

import { Check, Reply } from "lucide-react";

import { newSenderActions } from "@/features/modes/newSenderActions";
import type { ActionBehavior } from "@/lib/actions/types";

export const nowActions: ActionBehavior[] = [
  { id: "now.down", command: "down" },
  { id: "now.up", command: "up" },
  { id: "now.open", command: "open" },
  { id: "now.done", command: "done", icon: Check },
  { id: "now.reply", command: "reply", icon: Reply },
  { id: "now.open-email", command: "openEmail" },
  { id: "now.let-go-digest", command: "letGoDigest" },
  { id: "now.close-hint", command: "closeHint" },
  // Last, so the status strip (first five live keys) keeps the page's own verbs.
  ...newSenderActions("now"),
];
