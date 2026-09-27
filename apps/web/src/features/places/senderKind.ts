/*
 * One-key correction: move a sender to People, Reading, Paper trail or
 * Screened out (or back to automatic). The daemon remembers it for the
 * sender's future mail. The sender's bundle leaves the place it no longer
 * belongs to at once; Undo puts back the kind the daemon says it had.
 */

import type { QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import { getActiveQueryClient } from "@/lib/queryClient";

import { setSenderKind, type SenderKind } from "./api";
import { correctionMessage } from "./placeCopy";
import { mapPlaceData } from "./placePaging";

export interface SenderRef {
  accountId: string;
  senderEmail: string;
  /** How toasts name the sender. */
  label: string;
}

/** Take the sender's bundle out of every cached place it no longer belongs in. */
export function dropSenderFromPlaces(qc: QueryClient, sender: SenderRef, kind: SenderKind | null) {
  const email = sender.senderEmail.toLowerCase();
  // Automatic: the daemon decides where it goes; wait for the refetch.
  if (kind === null) return;
  qc.setQueriesData({ queryKey: ["place"] }, (data: unknown) =>
    mapPlaceData(data, (page) => {
      if (page.place === kind) return page;
      const bundles = page.bundles.filter(
        (bundle) =>
          !(bundle.account_id === sender.accountId && bundle.sender_email.toLowerCase() === email),
      );
      if (bundles.length === page.bundles.length) return page;
      const removed = page.bundles.length - bundles.length;
      return { ...page, bundles, total_bundles: Math.max(0, page.total_bundles - removed) };
    }),
  );
}

export async function correctSender(
  sender: SenderRef,
  kind: SenderKind | null,
  options: { announce?: boolean } = {},
): Promise<boolean> {
  const qc = getActiveQueryClient();
  const claim = options.announce === false ? null : claimUndo();
  if (qc) {
    await qc.cancelQueries({ queryKey: ["place"] });
    dropSenderFromPlaces(qc, sender, kind);
  }
  try {
    const set = await setSenderKind({ ...sender, kind });
    if (claim) {
      const previous = set.previous ?? null;
      claim.settle(
        offerUndo(
          correctionMessage(sender.label, kind),
          `sender-kind-${sender.accountId}-${sender.senderEmail}`,
          async () => {
            const undone = await correctSender(sender, previous, { announce: false });
            if (undone) toast.success(correctionMessage(sender.label, previous));
            return undone;
          },
          claim.run,
        ),
      );
    }
    return true;
  } catch (caught) {
    claim?.settle(null);
    toast.error(`Couldn't move ${sender.label}`, {
      description: caught instanceof Error ? caught.message : String(caught),
    });
    return false;
  } finally {
    await invalidateMailQueries(qc).catch(() => undefined);
  }
}
