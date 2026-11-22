import type { SearchGroupBy, SearchMode, SearchSort } from "./api";

export type Scope = "threads" | "messages" | "attachments" | "triage";
export type Verdict = "ACTION" | "FYI" | "ROUTINE";

export type SearchUpdate = Partial<{
  q: string;
  mode: SearchMode;
  sort: SearchSort | null;
  scope: Scope;
  verdict: Verdict | null;
  groupBy: SearchGroupBy;
  account: string | null;
}>;
