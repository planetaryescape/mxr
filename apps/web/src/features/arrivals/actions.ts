import { Shuffle, Users } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const arrivalActions: ActionBehavior[] = [
  { id: "modes.move", command: "moveToMode", icon: Shuffle },
  { id: "modes.move-sender", command: "moveSenderToMode", icon: Users },
  { id: "arrivals.down", command: "down" },
  { id: "arrivals.up", command: "up" },
  { id: "arrivals.open", command: "open" },
];
