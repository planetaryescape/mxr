/*
 * Compose-related palette actions.
 */

import { FileText, Mail } from "lucide-react";
import { toast } from "sonner";

import { listCommitments } from "@/features/mailbox/api";
import { getActiveQueryClient } from "@/lib/queryClient";
import type { Action } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

interface AccountSnapshot {
  account_id: string;
  enabled: boolean;
  is_default: boolean;
}

function defaultAccountId(): string | null {
  const client = getActiveQueryClient();
  if (!client) return null;
  const data = client.getQueryData<{ accounts?: AccountSnapshot[] }>(["accounts"]);
  if (!data?.accounts) return null;
  const def =
    data.accounts.find((a: AccountSnapshot) => a.enabled && a.is_default) ??
    data.accounts.find((a: AccountSnapshot) => a.enabled) ??
    data.accounts[0];
  return def?.account_id ?? null;
}

function closePalette(): void {
  useModals.getState().setCommandPaletteOpen(false);
}

export const composeActions: Action[] = [
  {
    id: "compose.draft-to",
    label: "Draft to...",
    description: "Pick a recipient, then use Draft for me",
    group: "Compose",
    icon: Mail,
    paletteOnly: true,
    run: () => {
      closePalette();
      useModals.getState().setComposeLauncherOpen(true);
    },
  },
  {
    id: "compose.show-commitments",
    label: "Show commitments...",
    description: "Open unresolved relationship commitments",
    group: "Compose",
    icon: FileText,
    paletteOnly: true,
    run: () => {
      closePalette();
      const accountId = defaultAccountId();
      if (!accountId) {
        toast.error("No account available");
        return;
      }
      listCommitments({ accountId, status: "open" })
        .then((result) => useModals.getState().openRightRail("commitments", result))
        .catch((error: Error) =>
          toast.error("Commitments unavailable", { description: error.message }),
        );
    },
  },
];
