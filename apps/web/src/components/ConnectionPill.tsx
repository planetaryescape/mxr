import { AlertTriangle, Cloud, CloudOff, Loader2 } from "lucide-react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useAllAccountsFreshness } from "@/features/freshness/api";
import { effectiveHealth, isCalm, syncLine, worstAccount } from "@/features/freshness/copy";
import { useClockLabel } from "@/lib/minuteClock";
import { useConnectionStore } from "@/state/connectionStore";
import { useDaemonDown } from "@/lib/daemonAvailability";
import { cn } from "@/lib/utils";

interface ConnectionPillProps {
  compact?: boolean;
}

export function ConnectionPill({ compact = false }: ConnectionPillProps) {
  const status = useConnectionStore((s) => s.state);
  const lastErrorAt = useConnectionStore((s) => s.lastErrorAt);
  const errorMessage = useConnectionStore((s) => s.errorMessage);
  const protocolMismatch = useConnectionStore((s) => s.protocolMismatch);
  const daemonDown = useDaemonDown();
  // Connected to the daemon is not the same as mail arriving: a failing or
  // stale account turns the pill amber and its tooltip says which.
  const { data: freshness } = useAllAccountsFreshness();
  const syncIssue = useClockLabel((now) => {
    const worst = freshness ? worstAccount(freshness, now) : undefined;
    if (!worst || !freshness) return "";
    return isCalm(effectiveHealth(worst, freshness.stale_after_secs, now))
      ? ""
      : syncLine(worst, freshness.stale_after_secs, now);
  });
  const syncWarning = status === "connected" && !protocolMismatch && syncIssue !== "";
  const Icon =
    daemonDown || protocolMismatch || syncWarning
      ? AlertTriangle
      : status === "connected"
        ? Cloud
        : status === "connecting" || status === "reconnecting"
          ? Loader2
          : CloudOff;
  const tone = daemonDown
    ? "text-warning"
    : protocolMismatch
      ? "text-destructive"
      : syncWarning
        ? "text-warning"
        : status === "connected"
          ? "text-success"
          : status === "connecting" || status === "reconnecting"
            ? "text-warning"
            : "text-destructive";
  const label = daemonDown
    ? "mxr unavailable"
    : protocolMismatch
      ? "protocol mismatch"
      : status === "connected"
        ? "connected"
        : status === "connecting"
          ? "connecting"
          : status === "reconnecting"
            ? "reconnecting"
            : status === "unauthorized"
              ? "needs local access"
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
        {daemonDown
          ? "Could not reach mxr. Check the connection, or run mxr restart on the computer running mxr."
          : protocolMismatch
            ? `IPC v${protocolMismatch.actualProtocol ?? "missing"}; expected v${protocolMismatch.requiredProtocol}`
            : errorMessage
              ? errorMessage
              : status === "connected"
                ? syncWarning
                  ? `Connected. ${syncIssue}`
                  : "Connected to mxr. Mail sync health is shown separately."
                : "Not connected"}
        {lastErrorAt ? (
          <div className="mt-1 opacity-60">
            last error: {new Date(lastErrorAt).toLocaleTimeString()}
          </div>
        ) : null}
      </TooltipContent>
    </Tooltip>
  );
}
