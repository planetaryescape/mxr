import { shortDuration } from "@/features/desk/deskCopy";

import type { OwedReplyRow } from "./api";

const DAY_SECONDS = 86_400;

export function waitingLine(row: OwedReplyRow): string {
  const waited = `Waiting ${shortDuration(row.waiting_days * DAY_SECONDS)}`;
  if (row.usual_seconds == null) return waited;
  return `${waited}; you usually reply within ${shortDuration(row.usual_seconds)}`;
}
