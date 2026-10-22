/*
 * Move to another mode (blueprint 22's key table, D119): `X` moves this
 * email, `K` sends everything from its sender. Each view's controller
 * decides what "this email" is. In the reader `K` stays the previous
 * message and Places keep their own `K` (Move sender to…), so there the
 * sender is a capital in the `X` picker.
 */

import { Shuffle, Users } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const arrivalActions: ActionBehavior[] = [
  { id: "modes.move", command: "moveToMode", icon: Shuffle },
  { id: "modes.move-sender", command: "moveSenderToMode", icon: Users },
];
