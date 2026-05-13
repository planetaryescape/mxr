/*
 * Rule + account creation palette actions.
 */

import { Plus } from "lucide-react";

import type { Action } from "@/lib/actions/types";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useModals } from "@/state/modalStore";

function navigate(to: string): () => void {
  return () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  };
}

export const rulesActions: Action[] = [
  {
    id: "rules.new",
    label: "New rule",
    description: "Open the deterministic rule builder",
    group: "Rules",
    icon: Plus,
    paletteOnly: true,
    run: navigate("/rules/new"),
  },
];
