import { LetGoAllDialog } from "@/features/updates/UpdatesDialogs";
import { letGo } from "@/features/updates/updatesVerbs";
import { useUiPrefs } from "@/state/uiPrefsStore";

import type { NowUpdatesCard } from "./api";

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
