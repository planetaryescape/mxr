import { useQuery } from "@tanstack/react-query";
import { Calendar, Check, HelpCircle, MoreHorizontal, X } from "lucide-react";

import { fetchInvites, type CalendarInviteData } from "./api";
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { Row, RowVerb } from "@/components/Row";
import { useLitRow, type LitRowProps } from "@/components/useLitRow";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatLongDate } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { CalendarMetadataView, CalendarPartstatView } from "@/features/mailbox/types";
import {
  openInviteComment,
  useInviteResponse,
  type InviteAction,
} from "@/features/invites/useInviteResponse";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useHeldPending } from "@/hooks/useDelayedPending";

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

function InviteRow({ invite, rowProps }: { invite: CalendarInviteData; rowProps: LitRowProps }) {
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

  const handleComment = (action: InviteAction) => openInviteComment(message_id, action);
  const secondary = [whenText(metadata), metadata.location, organizerText(metadata)]
    .filter(Boolean)
    .join(" · ");
  return (
    <Row
      accountId={invite.account_id}
      {...rowProps}
      lead={<Calendar aria-hidden className="size-4 text-text-3" />}
      primary={
        <span className={cancelled ? "line-through text-text-3" : undefined}>
          {metadata.summary || "(no title)"}
        </span>
      }
      secondary={secondary || undefined}
      trail={
        cancelled ? (
          <span className="text-danger">Cancelled</span>
        ) : metadata.is_update ? (
          <span className="text-cue">Updated</span>
        ) : viewerPartstat && !showActions ? (
          <span className={PARTSTAT_TONE[viewerPartstat]}>{PARTSTAT_LABELS[viewerPartstat]}</span>
        ) : null
      }
      verbs={
        showActions ? (
          <div role="group" aria-label="Invite response" className="flex items-center gap-1">
            <RowVerb
              label="Accept"
              Icon={Check}
              className={cn(pendingAction === "accept" && "text-text-1")}
              onClick={() => pendingAction === null && begin("accept")}
            />
            <RowVerb
              label="Tentative"
              Icon={HelpCircle}
              className={cn(pendingAction === "tentative" && "text-text-1")}
              onClick={() => pendingAction === null && begin("tentative")}
            />
            <RowVerb
              label="Decline"
              Icon={X}
              className={cn(pendingAction === "decline" && "text-text-1")}
              onClick={() => pendingAction === null && begin("decline")}
            />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="More invite actions"
                  className="text-text-2"
                >
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
        ) : undefined
      }
    />
  );
}

export function InvitesRoute() {
  const account = useUiPrefs((s) => s.accountScope) ?? undefined;
  const invites = useQuery({
    queryKey: ["invites", account ?? "all"],
    queryFn: () => fetchInvites(account),
  });
  const invitesHeld = useHeldPending(invites.isPending);
  const rows = invites.data?.invites ?? [];
  const { rowProps } = useLitRow(rows.map((invite) => invite.id));

  return (
    <Page
      title="Invites"
      description="Calendar invites found in your mail, across accounts. RSVPs send after a short undo window."
    >
      {invites.isPending || invitesHeld ? (
        <PageSkeleton rows={4} label="Loading invites" />
      ) : invites.isError ? (
        <PageError
          title="Invites unavailable"
          error={invites.error}
          onRetry={() => void invites.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          title="No calendar invites"
          body="Invites in synced mail show up here, ready to accept or decline."
        />
      ) : (
        <ul aria-label="Calendar invites">
          {rows.map((invite) => (
            <InviteRow key={invite.id} invite={invite} rowProps={rowProps(invite.id)} />
          ))}
        </ul>
      )}
    </Page>
  );
}
