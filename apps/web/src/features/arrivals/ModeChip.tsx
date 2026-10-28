import { createContext, useContext, type ReactNode } from "react";

import { ModeMark } from "@/components/ModeMark";
import { useLitReason } from "@/state/litReasonStore";

import { useArrivalModes } from "./api";
import type { ArrivalBucket, ArrivalItem, ModeId } from "./types";

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
 * Where an email went, as the mode's mark (docs/blueprint/26 "Show it's also
 * elsewhere"): the mode's glyph in the row's trail, with the daemon's line
 * ("→ Updates · automated sender") as its tooltip and accessible name. It
 * reads the stored assessment, so archived mail still says where it belongs.
 */
export function ModeChip({ messageId }: { messageId: string }) {
  const item = useContext(ChipContext)?.get(messageId);
  if (!item || !isMode(item.bucket)) return null;
  return (
    <span
      data-testid="mode-chip"
      data-bucket={item.bucket}
      aria-label={item.not_sure ? `${item.chip} · Not sure` : item.chip}
      title={item.chip}
      className="inline-flex shrink-0"
    >
      <ModeMark mode={item.bucket} />
      {item.not_sure ? <span className="type-meta ml-1 text-text-3">Not sure</span> : null}
    </span>
  );
}

function isMode(bucket: ArrivalBucket): bucket is ModeId {
  return (
    bucket !== "screened_out" && bucket !== "spam" && bucket !== "trash" && bucket !== "sorting"
  );
}

/**
 * The status bar's reason for the lit Inbox row: where it went and why, from
 * the same arrival line the mark carries. Renders nothing; it sits inside the
 * scope so it reads the chips already fetched for the visible rows.
 */
export function ModeChipReason({ messageId }: { messageId: string | undefined }) {
  const item = useContext(ChipContext)?.get(messageId ?? "");
  const line = item?.chip.replace(/^→\s*/, "");
  useLitReason(line ? `went to ${line}` : null, null, item?.account_id);
  return null;
}
