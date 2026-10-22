/*
 * Keys in Updates (built on Paper trail) and Reading. The mounted place view registers what
 * each command does (`useScopeController("place", …)`); the reader keeps
 * its own keys while it has focus.
 */

import { Archive, Check, Pin, Shuffle } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const placeActions: ActionBehavior[] = [
  { id: "place.down", command: "down" },
  { id: "place.up", command: "up" },
  { id: "place.open", command: "open" },
  { id: "place.done", command: "done", icon: Check },
  { id: "place.pin", command: "pin", icon: Pin },
  { id: "place.sweep-bundle", command: "sweepBundle", icon: Archive },
  { id: "place.sweep-all", command: "sweepAll", icon: Archive },
  { id: "place.move-sender", command: "moveSender", icon: Shuffle },
  { id: "place.unsubscribe", command: "unsubscribe" },
  { id: "reader.move-sender", command: "moveSender", icon: Shuffle },
];
