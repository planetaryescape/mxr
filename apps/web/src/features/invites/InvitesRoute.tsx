import { useQuery } from "@tanstack/react-query";
import { Calendar, Check, HelpCircle, MoreHorizontal, X } from "lucide-react";

import { fetchInvites, type CalendarInviteData } from "./api";
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatLongDate } from "@/lib/format";
import type { CalendarMetadataView, CalendarPartstatView } from "@/features/mailbox/types";
import {
  openInviteComment,
  useInviteResponse,
  type InviteAction,
} from "@/features/invites/useInviteResponse";
import { useUiPrefs } from "@/state/uiPrefsStore";

const PARTSTAT_LABELS: Record<CalendarPartstatView, string> = {
  accepted: "Accepted",
  tentative: "Tentative",
  declined: "Declined",
  needs_action: "Needs action",
  delegated: "Delegated",
};

const PARTSTAT_TONE: Record<CalendarPartstatView, string> = {
  accepted: "text-success",
  tentative: "text-warning",
  declined: "text-destructive",
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
  const start = formatLongDate(metadata.starts_at) || metadata.starts_at;
  if (!metadata.ends_at) return start;
  const end = new Date(metadata.ends_at);
  const sameDay = new Date(metadata.starts_at).toDateString() === end.toDateString();
  const endText = sameDay
    ? end.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    : formatLongDate(metadata.ends_at) || metadata.ends_at;
  return `${start} to ${endText}`;
}

function organizerText(metadata: CalendarMetadataView): string {
  const org = metadata.organizer;
  if (!org) return "";
  return org.name ? `${org.name} <${org.email}>` : org.email;
}

function InviteRow({ invite }: { invite: CalendarInviteData }) {
  const { metadata, message_id } = invite;
  const { begin, pendingAction } = useInviteResponse({ messageId: message_id });

  const cancelled = isCancelled(metadata);
  const viewerPartstat: CalendarPartstatView | null = metadata.viewer_partstat ?? null;
  const showActions =
    isRequest(metadata) &&
    !cancelled &&
    (viewerPartstat === null ||
      viewerPartstat === "needs_action" ||
      viewerPartstat === "delegated");

  const handleClick = (action: InviteAction) => begin(action);
  const handleComment = (action: InviteAction) => openInviteComment(message_id, action);

  return (
    <li className="flex items-start gap-3 border-b border-border/60 px-2 py-3">
      <div className="mt-0.5 text-muted-foreground">
        <Calendar className="size-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          {cancelled && (
            <span className="font-mono text-2xs uppercase tracking-wide text-destructive">
              Cancelled
            </span>
          )}
          {!cancelled && metadata.is_update && (
            <span className="font-mono text-2xs uppercase tracking-wide text-warning">Updated</span>
          )}
          <span
            className={
              cancelled
                ? "truncate text-[13px] font-medium line-through text-muted-foreground"
                : "truncate text-[13px] font-medium"
            }
          >
            {metadata.summary || "(no title)"}
          </span>
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-muted-foreground">
          {whenText(metadata) && (
            <span className={cancelled ? "line-through" : ""}>{whenText(metadata)}</span>
          )}
          {metadata.location && <span>· {metadata.location}</span>}
          {organizerText(metadata) && <span>· {organizerText(metadata)}</span>}
        </div>

        {showActions ? (
          <div
            className="mt-2 flex flex-wrap items-center gap-2"
            role="group"
            aria-label="Invite response"
          >
            <Button
              variant={pendingAction === "accept" ? "default" : "outline"}
              size="xs"
              onClick={() => handleClick("accept")}
              disabled={pendingAction !== null && pendingAction !== "accept"}
            >
              <Check className="size-3" /> Accept
            </Button>
            <Button
              variant={pendingAction === "tentative" ? "default" : "outline"}
              size="xs"
              onClick={() => handleClick("tentative")}
              disabled={pendingAction !== null && pendingAction !== "tentative"}
            >
              <HelpCircle className="size-3" /> Tentative
            </Button>
            <Button
              variant={pendingAction === "decline" ? "default" : "outline"}
              size="xs"
              onClick={() => handleClick("decline")}
              disabled={pendingAction !== null && pendingAction !== "decline"}
            >
              <X className="size-3" /> Decline
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="xs" aria-label="More invite actions">
                  <MoreHorizontal className="size-3.5" />
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
            <p className={`mt-1.5 font-mono text-2xs ${PARTSTAT_TONE[viewerPartstat]}`}>
              {PARTSTAT_LABELS[viewerPartstat]}
            </p>
          )
        )}
      </div>
    </li>
  );
}

export function InvitesRoute() {
  const account = useUiPrefs((s) => s.accountScope) ?? undefined;
  const invites = useQuery({
    queryKey: ["invites", account ?? "all"],
    queryFn: () => fetchInvites(account),
  });
  const rows = invites.data?.invites ?? [];

  return (
    <Page
      title="Invites"
      description="Calendar invites found in your mail, across accounts. RSVPs send after a short undo window."
    >
      {invites.isPending ? (
        <PageSkeleton rows={4} label="Loading invites" />
      ) : invites.isError ? (
        <PageError
          title="Invites unavailable"
          error={invites.error}
          onRetry={() => void invites.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          icon={<Calendar className="size-5" />}
          title="No calendar invites"
          body="Invites in synced mail show up here, ready to accept or decline."
        />
      ) : (
        <RuledList label="Calendar invites">
          {rows.map((invite) => (
            <InviteRow key={invite.id} invite={invite} />
          ))}
        </RuledList>
      )}
    </Page>
  );
}
