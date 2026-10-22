/*
 * Rule + account creation palette actions.
 */

import { Plus } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useModals } from "@/state/modalStore";

function navigate(to: string): () => void {
  return () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  };
}

export const rulesActions: ActionBehavior[] = [
  { id: "rules.new", icon: Plus, run: navigate("/rules/new") },
];
