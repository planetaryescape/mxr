import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { DraftSuggestionResponse } from "./types";

export function DraftQualityBadges({
  suggestion,
  compact = false,
}: {
  suggestion: DraftSuggestionResponse | null;
  compact?: boolean;
}) {
  if (!suggestion) return null;
  return (
    <div className={cn("flex flex-wrap items-center gap-1.5", compact && "text-2xs")}>
      {suggestion.voice_match ? (
        <Badge variant="outline" className="bg-background">
          Voice {Math.round(suggestion.voice_match.score * 100)}% ·{" "}
          {suggestion.voice_match.confidence}
        </Badge>
      ) : null}
      {suggestion.humanizer ? (
        <Badge variant="outline" className="bg-background">
          Humanizer {suggestion.humanizer.score}/100
        </Badge>
      ) : null}
      {suggestion.rewrite_iterations > 0 ? (
        <Badge variant="secondary">Rewritten {suggestion.rewrite_iterations}x</Badge>
      ) : null}
    </div>
  );
}
