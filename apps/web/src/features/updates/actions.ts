/*
 * Updates' keys, from blueprint 22's key table and the daemon's Updates
 * guide. The mounted briefing registers what each command does
 * (`useScopeController("updates", …)`); the reader keeps its own keys
 * while it has focus. `g u` opens Updates from anywhere.
 */

import { Archive, BellOff, Check, Copy, ExternalLink, ListPlus, Mail, Pin } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const updatesActions: ActionBehavior[] = [
  { id: "updates.down", command: "down" },
  { id: "updates.up", command: "up" },
  { id: "updates.expand", command: "expand" },
  { id: "updates.let-go-all", command: "letGoAll", icon: Check },
  { id: "updates.let-go-source", command: "letGoSource" },
  { id: "updates.needs-me", command: "needsMe", icon: ListPlus },
  { id: "updates.tune", command: "tune", icon: BellOff },
  { id: "updates.link", command: "link", icon: ExternalLink },
  { id: "updates.copy-code", command: "copyCode", icon: Copy },
  { id: "updates.open-email", command: "openEmail", icon: Mail },
  { id: "updates.peek", command: "peek" },
  { id: "updates.pin", command: "pin", icon: Pin },
  { id: "updates.sweep", command: "sweep", icon: Archive },
  { id: "updates.close-hint", command: "closeHint" },
];
