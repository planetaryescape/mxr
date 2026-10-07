import { createContext, useContext, type ReactNode } from "react";

import { useArrivalModes } from "./api";
import { bucketName } from "./arrivalLine";
import type { ArrivalItem } from "./types";

const ChipContext = createContext<ReadonlyMap<string, ArrivalItem> | undefined>(undefined);

/**
 * Inbox's chips for the rows on screen, in one request per range. Lists
 * without chips never mount the query.
 */
export function ModeChipScope({
  enabled,
  messageIds,
  children,
}: {
  enabled: boolean;
  messageIds: readonly string[];
  children: ReactNode;
}) {
  if (!enabled) return children;
  return <ChipQuery messageIds={messageIds}>{children}</ChipQuery>;
}

function ChipQuery({
  messageIds,
  children,
}: {
  messageIds: readonly string[];
  children: ReactNode;
}) {
  const chips = useArrivalModes(messageIds).data;
  return <ChipContext.Provider value={chips}>{children}</ChipContext.Provider>;
}

/**
 * Inbox's quiet note of where an email went: the mode's name, with the
 * reason on hover and for screen readers ("→ Updates · automated sender").
 * It reads the arrival as stored, so archived mail still says where it went.
 */
export function ModeChip({ messageId }: { messageId: string }) {
  const item = useContext(ChipContext)?.get(messageId);
  if (!item) return null;
  return (
    <span
      data-testid="mode-chip"
      title={item.chip}
      aria-label={item.chip}
      className="whitespace-nowrap text-[length:var(--mail-row-meta-size)] text-muted-foreground"
    >
      {bucketName(item.bucket)}
    </span>
  );
}
