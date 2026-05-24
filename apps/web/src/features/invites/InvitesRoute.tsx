import { useQuery } from "@tanstack/react-query";
import { Calendar, Check, HelpCircle, MoreHorizontal, RefreshCw, X } from "lucide-react";
import { toast } from "sonner";

import { fetchInvites, type CalendarInviteData } from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { CalendarMetadataView, CalendarPartstatView } from "@/features/mailbox/types";
import {
  openInviteReplyComposeSession,
  useInviteResponse,
  type InviteAction,
} from "@/features/thread/useInviteResponse";

const PARTSTAT_LABELS: Record<CalendarPartstatView, string> = {
  accepted: "Accepted",
  tentative: "Tentative",
  declined: "Declined",
  needs_action: "Needs action",
  delegated: "Delegated",
};

const PARTSTAT_TONE: Record<CalendarPartstatView, string> = {
  accepted: "text-emerald-600",
  tentative: "text-amber-600",
  declined: "text-red-600",
  needs_action: "text-muted-foreground",
  delegated: "text-muted-foreground",
};

function isCancelled(metadata: CalendarMetadataView): boolean {
  return (
    (metadata.method ?? "").toUpperCase() === "CANCEL" ||
    (metadata.status ?? "").toUpperCase() === "CANCELLED"
  );
}

function isRequest(metadata: CalendarMetadataView): boolean {
  const method = (metadata.method ?? "").toUpperCase();
  return method === "" || method === "REQUEST";
}

function whenText(metadata: CalendarMetadataView): string {
  if (!metadata.starts_at) return "";
  return metadata.ends_at
    ? `${metadata.starts_at} – ${metadata.ends_at}`
    : metadata.starts_at;
}

function organizerText(metadata: CalendarMetadataView): string {
  const org = metadata.organizer;
  if (!org) return "";
  return org.name ? `${org.name} <${org.email}>` : org.email;
}

function InviteRow({ invite }: { invite: CalendarInviteData }) {
  const { metadata, message_id } = invite;
  const { begin, cancel, pendingAction } = useInviteResponse({
    messageId: message_id,
    invalidateKeys: [["invites"]],
  });

  const cancelled = isCancelled(metadata);
  const viewerPartstat: CalendarPartstatView | null = metadata.viewer_partstat ?? null;
  const showActions =
    isRequest(metadata) &&
    !cancelled &&
    (viewerPartstat === null ||
      viewerPartstat === "needs_action" ||
      viewerPartstat === "delegated");

  const handleClick = (action: InviteAction) => {
    begin(action);
    toast(
      action === "accept"
        ? "Accepting invite…"
        : action === "tentative"
          ? "Responding tentative…"
          : "Declining invite…",
      {
        id: `invite-${message_id}`,
        duration: 1100,
        action: {
          label: "Undo",
          onClick: () => {
            cancel();
            toast.success("Cancelled", { id: `invite-${message_id}`, duration: 1500 });
          },
        },
      },
    );
  };

  const handleComment = async (action: InviteAction) => {
    try {
      await openInviteReplyComposeSession(message_id, action);
      toast.success("Compose draft opened — write your comment then send");
    } catch (error) {
      toast.error("Failed to open comment compose", { description: String(error) });
    }
  };

  return (
    <div className="flex items-start gap-4 border-b border-border px-6 py-4">
      <div className="mt-0.5 text-muted-foreground">
        <Calendar className="size-5" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          {cancelled && (
            <span className="rounded bg-red-500/10 px-2 py-0.5 font-mono text-2xs uppercase tracking-wide text-red-600">
              Cancelled
            </span>
          )}
          {!cancelled && metadata.is_update && (
            <span className="rounded bg-amber-500/10 px-2 py-0.5 font-mono text-2xs uppercase tracking-wide text-amber-600">
              Updated
            </span>
          )}
          <span
            className={cancelled ? "truncate font-medium line-through text-muted-foreground" : "truncate font-medium"}
          >
            {metadata.summary || "(no title)"}
          </span>
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-muted-foreground">
          {whenText(metadata) && <span className={cancelled ? "line-through" : ""}>{whenText(metadata)}</span>}
          {metadata.location && <span>· {metadata.location}</span>}
          {organizerText(metadata) && <span>· {organizerText(metadata)}</span>}
        </div>

        {showActions ? (
          <div className="mt-2 flex flex-wrap items-center gap-2" role="group" aria-label="Invite response">
            <Button
              variant={pendingAction === "accept" ? "default" : "outline"}
              size="sm"
              onClick={() => handleClick("accept")}
              disabled={pendingAction !== null && pendingAction !== "accept"}
            >
              <Check className="mr-1 h-4 w-4" /> Accept
            </Button>
            <Button
              variant={pendingAction === "tentative" ? "default" : "outline"}
              size="sm"
              onClick={() => handleClick("tentative")}
              disabled={pendingAction !== null && pendingAction !== "tentative"}
            >
              <HelpCircle className="mr-1 h-4 w-4" /> Tentative
            </Button>
            <Button
              variant={pendingAction === "decline" ? "default" : "outline"}
              size="sm"
              onClick={() => handleClick("decline")}
              disabled={pendingAction !== null && pendingAction !== "decline"}
            >
              <X className="mr-1 h-4 w-4" /> Decline
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" aria-label="More invite actions">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => handleComment("accept")}>
                  Accept with comment
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => handleComment("tentative")}>
                  Maybe with comment
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => handleComment("decline")}>
                  Decline with comment
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ) : (
          !cancelled &&
          viewerPartstat && (
            <p className={`mt-2 text-sm font-semibold ${PARTSTAT_TONE[viewerPartstat]}`}>
              {PARTSTAT_LABELS[viewerPartstat]}
            </p>
          )
        )}
      </div>
    </div>
  );
}

export function InvitesRoute() {
  const invites = useQuery({ queryKey: ["invites"], queryFn: fetchInvites });
  const rows = invites.data?.invites ?? [];

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Calendar
        </div>
        <h1 className="text-xl font-semibold tracking-tight">Calendar invites</h1>
        <p className="mt-1 text-2xs text-muted-foreground">
          Every calendar invite detected in your mail, across accounts. Respond inline — RSVPs
          send after a short undo window.
        </p>
      </header>

      {invites.isLoading ? (
        <div className="p-6 text-xs text-muted-foreground">Loading invites…</div>
      ) : invites.isError ? (
        <EmptyState
          icon={RefreshCw}
          title="Invites unavailable"
          description={invites.error.message}
          action={<Button onClick={() => invites.refetch()}>Retry</Button>}
        />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={Calendar}
          title="No calendar invites"
          description="Invites detected in synced mail will appear here."
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          {rows.map((invite) => (
            <InviteRow key={invite.id} invite={invite} />
          ))}
        </div>
      )}
    </div>
  );
}
