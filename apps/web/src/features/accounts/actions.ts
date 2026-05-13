/*
 * Account creation palette action.
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

export const accountsActions: Action[] = [
  {
    id: "accounts.new",
    label: "Add account",
    description: "Open account onboarding",
    group: "Accounts",
    icon: Plus,
    paletteOnly: true,
    run: navigate("/accounts/new"),
  },
];
