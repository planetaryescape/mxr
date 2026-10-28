/*
 * Move to another mode (blueprint 35, D119): `X` moves this email, `P`
 * sends everything from its sender. Each view's controller decides what
 * "this email" is. Updates and Reading keep their own `P`, so there the
 * sender is a capital in the `X` picker.
 */

import { Shuffle, Users } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const arrivalActions: ActionBehavior[] = [
  { id: "modes.move", command: "moveToMode", icon: Shuffle },
  { id: "modes.move-sender", command: "moveSenderToMode", icon: Users },
  // The arrivals page (`ArrivalsRoute`), which registers the commands.
  { id: "arrivals.down", command: "down" },
  { id: "arrivals.up", command: "up" },
  { id: "arrivals.open", command: "open" },
];
