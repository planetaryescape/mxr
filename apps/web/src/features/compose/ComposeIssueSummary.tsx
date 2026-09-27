/*
 * One compact line of compose validation, shown next to the send bar.
 * Blocking issues (send refused) and warnings (send allowed) stay visually
 * distinct: different icon, colour and lead-in.
 */

import { AlertTriangle, CircleX } from "lucide-react";

import type { ComposeIssue } from "./api";

export function ComposeIssueSummary({ issues }: { issues: ComposeIssue[] }) {
  const blocking = issues.filter((issue) => issue.severity === "error");
  const warnings = issues.filter((issue) => issue.severity !== "error");
  if (blocking.length === 0 && warnings.length === 0) return null;
  return (
    <div
      role="status"
      aria-label="Compose issues"
      className="flex flex-wrap items-center gap-x-4 gap-y-1 text-2xs"
    >
      {blocking.length > 0 ? (
        <span className="flex min-w-0 items-center gap-1.5 text-destructive">
          <CircleX className="size-3 shrink-0" aria-hidden="true" />
          <span className="font-medium">Can&apos;t send yet:</span>
          <span className="min-w-0">{blocking.map((issue) => issue.message).join("; ")}</span>
        </span>
      ) : null}
      {warnings.length > 0 ? (
        <span className="flex min-w-0 items-center gap-1.5 text-warning">
          <AlertTriangle className="size-3 shrink-0" aria-hidden="true" />
          <span className="font-medium">Check:</span>
          <span className="min-w-0">{warnings.map((issue) => issue.message).join("; ")}</span>
        </span>
      ) : null}
    </div>
  );
}
