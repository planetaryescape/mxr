/*
 * Move to another mode (blueprint 22's key table, D119): `X` moves this
 * email, `K` sends everything from its sender. Each view's controller
 * decides what "this email" is. In the reader `K` stays the previous
 * message and Places keep their own `K` (Move sender to…), so there the
 * sender is a capital in the `X` picker.
 */

import { Shuffle, Users } from "lucide-react";

import type { Action } from "@/lib/actions/types";

export const arrivalActions: Action[] = [
  {
    id: "modes.move",
    command: "moveToMode",
    label: "Move to…",
    shortLabel: "Move",
    description: "This email only: Messages, To do, Updates, Reading or Archive",
    shortcut: "X",
    group: "Triage",
    icon: Shuffle,
    scopes: ["now", "messages", "list", "reader", "place", "updates", "reading"],
  },
  {
    id: "modes.move-sender",
    command: "moveSenderToMode",
    label: "Send everything from this sender to…",
    shortLabel: "Move sender",
    description: "Messages, Updates or Reading, for their mail from now on",
    shortcut: "K",
    group: "Triage",
    icon: Users,
    scopes: ["now", "messages", "list"],
  },
];
