import { Check, MoreHorizontal, X, HelpCircle } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type {
  CalendarMetadataView,
  CalendarPartstatView,
} from "@/features/mailbox/types";
import { cn } from "@/lib/utils";

import {
  openInviteReplyComposeSession,
  useInviteResponse,
  type InviteAction,
} from "./useInviteResponse";
import { useLocale } from "./useLocale";

interface InviteCardProps {
  messageId: string;
  threadId: string;
  metadata: CalendarMetadataView;
  className?: string;
}

function statusFromMethod(method: string | null | undefined): string {
  return (method ?? "").toUpperCase();
}

function isCancelled(metadata: CalendarMetadataView): boolean {
  return (
    statusFromMethod(metadata.method) === "CANCEL" ||
    (metadata.status ?? "").toUpperCase() === "CANCELLED"
  );
}

function isPublish(metadata: CalendarMetadataView): boolean {
  return statusFromMethod(metadata.method) === "PUBLISH";
}

function isCounter(metadata: CalendarMetadataView): boolean {
  return statusFromMethod(metadata.method) === "COUNTER";
}

function hasParseWarning(metadata: CalendarMetadataView): boolean {
  return (metadata.warnings ?? []).some((w) =>
    w.toLowerCase().includes("could not be parsed"),
  );
}

function actionableWarnings(metadata: CalendarMetadataView): string[] {
  return (metadata.warnings ?? []).filter(
    (w) => !w.toLowerCase().includes("could not be parsed"),
  );
}

/// State-driven calendar-invite card rendered above the message body. Mirrors
/// the TUI variant in `crates/tui/src/ui/message_view.rs` so both surfaces
/// stay in lockstep — change one, change the other.
export function InviteCard({
  messageId,
  threadId,
  metadata,
  className,
}: InviteCardProps): React.ReactElement {
  const locale = useLocale();
  const { begin, cancel, pendingAction } = useInviteResponse({
    messageId,
    threadId,
  });

  const cancelled = isCancelled(metadata);
  const publish = isPublish(metadata);
  const counter = isCounter(metadata);
  const parseFailed = hasParseWarning(metadata);
  const isRequest =
    !metadata.method ||
    statusFromMethod(metadata.method) === "REQUEST";
  const warnings = actionableWarnings(metadata);

  const handleClick = (action: InviteAction) => {
    begin(action);
    const message =
      action === "accept"
        ? locale.status.invite_pending_accept
        : action === "tentative"
        ? locale.status.invite_pending_tentative
        : locale.status.invite_pending_decline;
    toast(message, {
      id: `invite-${messageId}`,
      duration: 1100,
      action: {
        label: "Undo",
        onClick: () => {
          cancel();
          toast.success(locale.status.invite_cancelled, {
            id: `invite-${messageId}`,
            duration: 1500,
          });
        },
      },
    });
  };

  const handleComment = async (action: InviteAction) => {
    try {
      const session = (await openInviteReplyComposeSession(
        messageId,
        action,
      )) as { draftPath?: string; id?: string } | undefined;
      if (session) {
        toast.success("Compose draft opened — write your comment then send");
      }
    } catch (error) {
      toast.error("Failed to open comment compose", {
        description: String(error),
      });
    }
  };

  const viewerPartstat: CalendarPartstatView | null =
    metadata.viewer_partstat ?? null;

  return (
    <article
      role="region"
      aria-label="Calendar invite"
      className={cn(
        "rounded-lg border bg-card text-card-foreground shadow-sm p-4 mb-4",
        cancelled && "border-red-500/40",
        !cancelled && metadata.is_update && "border-amber-500/40",
        !cancelled && !metadata.is_update && !viewerPartstat && "border-primary/40",
        className,
      )}
    >
      <header className="flex items-baseline justify-between mb-2">
        <span className="text-sm font-medium text-muted-foreground">
          {locale.invite.card_title}
        </span>
        {metadata.is_update && !cancelled && (
          <span className="text-xs font-medium text-amber-600">
            {locale.invite.banner_updated}
          </span>
        )}
      </header>

      <h3
        className={cn(
          "text-base font-semibold",
          cancelled && "line-through text-muted-foreground",
        )}
      >
        {metadata.summary ?? locale.invite.card_title}
      </h3>

      <dl className="mt-2 text-sm grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        {metadata.starts_at && (
          <>
            <dt className="text-muted-foreground">When</dt>
            <dd className={cn(cancelled && "line-through")}>
              {metadata.starts_at}
              {metadata.ends_at ? ` – ${metadata.ends_at}` : ""}
            </dd>
          </>
        )}
        {metadata.location && (
          <>
            <dt className="text-muted-foreground">Where</dt>
            <dd className={cn(cancelled && "line-through")}>{metadata.location}</dd>
          </>
        )}
        {metadata.rrule && (
          <>
            <dt className="text-muted-foreground">Repeats</dt>
            <dd>{metadata.rrule}</dd>
          </>
        )}
        {metadata.organizer && (
          <>
            <dt className="text-muted-foreground">Org</dt>
            <dd>
              {metadata.organizer.name
                ? `${metadata.organizer.name} <${metadata.organizer.email}>`
                : metadata.organizer.email}
            </dd>
          </>
        )}
      </dl>

      {warnings.length > 0 && (
        <p className="mt-2 text-xs text-amber-700">{warnings.join("; ")}</p>
      )}

      {cancelled && (
        <p
          role="status"
          aria-live="polite"
          className="mt-3 text-sm font-semibold text-red-600"
        >
          {locale.invite.banner_cancelled}
        </p>
      )}

      {parseFailed && (
        <p
          role="status"
          aria-live="polite"
          className="mt-3 text-sm font-semibold text-red-600"
        >
          {locale.invite.banner_parse_warning}
        </p>
      )}

      {counter && (
        <p className="mt-3 text-sm font-semibold text-amber-700">
          {locale.invite.banner_counter}
        </p>
      )}

      {publish && (
        <p className="mt-3 text-sm text-muted-foreground">
          {locale.invite.banner_publish}
        </p>
      )}

      {isRequest && !cancelled && !parseFailed && (
        <div className="mt-3">
          {viewerPartstat === "accepted" && (
            <ResponseStateRow
              text={locale.invite.state_label_accepted}
              hint={locale.invite.hint_change_response}
              tone="success"
            />
          )}
          {viewerPartstat === "tentative" && (
            <ResponseStateRow
              text={locale.invite.state_label_tentative}
              hint={locale.invite.hint_change_response}
              tone="warning"
            />
          )}
          {viewerPartstat === "declined" && (
            <ResponseStateRow
              text={locale.invite.state_label_declined}
              hint={locale.invite.hint_change_response}
              tone="danger"
            />
          )}
          {(viewerPartstat === null ||
            viewerPartstat === "needs_action" ||
            viewerPartstat === "delegated") && (
            <ActionRow
              locale={locale}
              pendingAction={pendingAction}
              onClick={handleClick}
              onComment={handleComment}
            />
          )}
        </div>
      )}
    </article>
  );
}

interface ResponseStateRowProps {
  text: string;
  hint: string;
  tone: "success" | "warning" | "danger";
}

function ResponseStateRow({
  text,
  hint,
  tone,
}: ResponseStateRowProps): React.ReactElement {
  const toneClass =
    tone === "success"
      ? "text-emerald-600"
      : tone === "warning"
      ? "text-amber-600"
      : "text-red-600";
  return (
    <div role="status" aria-live="polite">
      <p className={cn("text-sm font-semibold", toneClass)}>{text}</p>
      <p className="text-xs text-muted-foreground mt-0.5">{hint}</p>
    </div>
  );
}

interface ActionRowProps {
  locale: ReturnType<typeof useLocale>;
  pendingAction: InviteAction | null;
  onClick: (action: InviteAction) => void;
  onComment: (action: InviteAction) => void;
}

function ActionRow({
  locale,
  pendingAction,
  onClick,
  onComment,
}: ActionRowProps): React.ReactElement {
  return (
    <div
      role="group"
      aria-label="Invite response"
      className="flex flex-wrap items-center gap-2"
    >
      <Button
        variant={pendingAction === "accept" ? "default" : "outline"}
        size="sm"
        onClick={() => onClick("accept")}
        disabled={pendingAction !== null && pendingAction !== "accept"}
      >
        <Check className="mr-1 h-4 w-4" /> {locale.invite.chip_label_accept}
      </Button>
      <Button
        variant={pendingAction === "tentative" ? "default" : "outline"}
        size="sm"
        onClick={() => onClick("tentative")}
        disabled={pendingAction !== null && pendingAction !== "tentative"}
      >
        <HelpCircle className="mr-1 h-4 w-4" />{" "}
        {locale.invite.chip_label_tentative}
      </Button>
      <Button
        variant={pendingAction === "decline" ? "default" : "outline"}
        size="sm"
        onClick={() => onClick("decline")}
        disabled={pendingAction !== null && pendingAction !== "decline"}
      >
        <X className="mr-1 h-4 w-4" /> {locale.invite.chip_label_decline}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" aria-label="More invite actions">
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => onComment("accept")}>
            Accept with comment
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => onComment("tentative")}>
            Maybe with comment
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => onComment("decline")}>
            Decline with comment
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <span className="ml-auto text-xs text-muted-foreground">
        {locale.invite.hint_comment}
      </span>
    </div>
  );
}
