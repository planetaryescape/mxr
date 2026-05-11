import { Cloud, CloudOff, Loader2 } from "lucide-react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useConnectionStore } from "@/state/connectionStore";
import { cn } from "@/lib/utils";

interface ConnectionPillProps {
  compact?: boolean;
}

export function ConnectionPill({ compact = false }: ConnectionPillProps) {
  const status = useConnectionStore((s) => s.state);
  const lastErrorAt = useConnectionStore((s) => s.lastErrorAt);
  const errorMessage = useConnectionStore((s) => s.errorMessage);
  const Icon =
    status === "connected"
      ? Cloud
      : status === "connecting" || status === "reconnecting"
        ? Loader2
        : CloudOff;
  const tone =
    status === "connected"
      ? "text-success"
      : status === "connecting" || status === "reconnecting"
        ? "text-warning"
        : "text-destructive";
  const label =
    status === "connected"
      ? "connected"
      : status === "connecting"
        ? "connecting"
        : status === "reconnecting"
          ? "reconnecting"
          : status === "unauthorized"
            ? "no token"
            : "offline";
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn(
            "inline-flex items-center gap-1 font-mono text-2xs",
            tone,
            compact && "justify-center",
          )}
          aria-label={compact ? `Connection: ${label}` : undefined}
        >
          <Icon
            className={cn(
              "size-3",
              (status === "connecting" || status === "reconnecting") && "animate-spin",
            )}
          />
          {!compact ? label : null}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        {errorMessage ? errorMessage : status === "connected" ? "WebSocket attached" : "—"}
        {lastErrorAt ? (
          <div className="mt-1 opacity-60">
            last error: {new Date(lastErrorAt).toLocaleTimeString()}
          </div>
        ) : null}
      </TooltipContent>
    </Tooltip>
  );
}
