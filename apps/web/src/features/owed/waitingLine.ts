import { shortDuration } from "@/features/desk/deskCopy";

import type { OwedReplyRow } from "./api";

const DAY_SECONDS = 86_400;

/**
 * "Waiting 3d; you usually reply within 4h", from the daemon's numbers. The
 * usual pace shows only when your reply history gives one.
 */
export function waitingLine(row: OwedReplyRow): string {
  const waited = `Waiting ${shortDuration(row.waiting_days * DAY_SECONDS)}`;
  if (row.usual_seconds == null) return waited;
  return `${waited}; you usually reply within ${shortDuration(row.usual_seconds)}`;
}
