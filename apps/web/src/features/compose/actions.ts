/*
 * Compose-related palette actions, plus the composer's own chords
 * (`composeKeyActions`), which compose runs itself.
 */

import { FileText, Mail } from "lucide-react";
import { toast } from "sonner";

import { listCommitments } from "@/features/mailbox/api";
import { getActiveQueryClient } from "@/lib/queryClient";
import type { ActionBehavior } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

import { composeKeyActions } from "./session/composeShortcuts";

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

export const composeActions: ActionBehavior[] = [
  {
    id: "compose.draft-to",
    icon: Mail,
    run: () => {
      closePalette();
      useModals.getState().setComposeLauncherOpen(true);
    },
  },
  {
    id: "compose.show-commitments",
    icon: FileText,
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
  ...composeKeyActions,
];
