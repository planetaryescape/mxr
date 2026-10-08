import { LetGoAllDialog } from "@/features/updates/UpdatesDialogs";
import { letGo } from "@/features/updates/updatesVerbs";
import { useUiPrefs } from "@/state/uiPrefsStore";

import type { NowUpdatesCard } from "./api";

/**
 * Let go of the Updates card: the same dry run and commit as `A` in
 * Updates, for the cut the card shows. What stays in To do is named
 * before you confirm, and Undo puts it all back.
 */
export function LetGoDigestDialog({
  card,
  open,
  onOpenChange,
}: {
  card: NowUpdatesCard;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const account = useUiPrefs((s) => s.accountScope);
  return (
    <LetGoAllDialog
      open={open}
      onOpenChange={onOpenChange}
      account={account}
      cut={card.since}
      onConfirm={(selectionToken) => void letGo({ account, cut: card.since, selectionToken }, [])}
    />
  );
}
