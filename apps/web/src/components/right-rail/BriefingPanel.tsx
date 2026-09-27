/* Thread or recipient briefing with its cited sources. */

import { Badge } from "@/components/ui/badge";
import { extractBriefing } from "./railPayloads";
import { formatDate, formatShortDate } from "./railFormat";

export function BriefingPanel({ payload }: { payload: unknown }) {
  const briefing = extractBriefing(payload);
  if (!briefing) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-4 text-sm text-foreground">
        No briefing available.
      </div>
    );
  }
  return (
    <div className="space-y-3 text-foreground">
      <div className="flex items-center gap-2">
        <Badge variant={briefing.from_cache ? "secondary" : "outline"}>
          {briefing.from_cache ? "Cached" : "Fresh"}
        </Badge>
        <span className="text-2xs text-muted-foreground">{formatDate(briefing.generated_at)}</span>
      </div>
      {/* Reader-first: render the markdown body as plain wrapped text rather
          than pulling in a markdown renderer the app doesn't already ship. */}
      <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-foreground">
        {briefing.body_markdown.trim() || "No briefing content."}
      </p>
      {briefing.citations && briefing.citations.length > 0 ? (
        <div className="space-y-1.5 rounded-md border border-border bg-muted/30 p-3">
          <h4 className="text-xs font-medium">Sources</h4>
          <ul className="space-y-1">
            {briefing.citations.slice(0, 12).map((citation, index) => (
              <li
                key={citation.message_id ?? index}
                className="truncate text-2xs text-muted-foreground"
                title={citation.subject ?? citation.message_id}
              >
                {citation.subject?.trim() || citation.message_id || "(source)"}
                {citation.date ? ` · ${formatShortDate(citation.date)}` : ""}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
