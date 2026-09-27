import { plural } from "@/lib/format";

import type { OwedReplyRow } from "./api";

/** "Waiting 3 days; you usually reply within 1 day", from the daemon's numbers. */
export function waitingLine(row: OwedReplyRow): string {
  const waited = `Waiting ${plural(Math.round(row.waiting_days), "day")}`;
  if (!row.expected_days) return waited;
  return `${waited}; you usually reply within ${plural(Math.round(row.expected_days), "day")}`;
}
