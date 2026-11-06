/* URL search params the search route reads and writes. */

import type { SearchGroupBy, SearchMode, SearchSort } from "./api";

export type Scope = "threads" | "messages" | "attachments" | "triage";
export type Verdict = "ACTION" | "FYI" | "ROUTINE";

/** A partial change to the search params; `null` clears an optional one. */
export type SearchUpdate = Partial<{
  q: string;
  mode: SearchMode;
  /** `null` returns to Default, leaving the order to the daemon. */
  sort: SearchSort | null;
  scope: Scope;
  verdict: Verdict | null;
  groupBy: SearchGroupBy;
  account: string | null;
}>;
