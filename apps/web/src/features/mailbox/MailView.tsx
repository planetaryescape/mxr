import { Link, Navigate } from "@tanstack/react-router";
import { Inbox, RefreshCw, Rows3, SearchX, SquareStack } from "lucide-react";
import { useMemo } from "react";

import { syncNow } from "./actions";
import { lensesFromShell, resolveLens, type LensRoute, type MailLens } from "./lenses";
import { ListWithReader } from "./ListWithReader";
import { Centered, ListSkeleton } from "./MailViewParts";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useMailboxQuery, useShellQuery } from "./useMailboxQuery";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { formatRelative, plural } from "@/lib/format";
import { useConnectionStore } from "@/state/connectionStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export { Centered, ListSkeleton };

/** System mailboxes that are pages of their own, not label lenses. */
const PAGE_MAILBOXES: Record<string, string> = {
  snoozed: "/snoozed",
  drafts: "/drafts",
  "reply-later": "/reply-queue",
  subscriptions: "/subscriptions",
};

/** A mailbox lens (system mailbox, label or saved search) with its reader. */
export function MailView({ route }: { route: LensRoute }) {
  const shell = useShellQuery();
  const lenses = useMemo(() => lensesFromShell(shell.data), [shell.data]);
  const phase = useDelayedPending(shell.isLoading);

  if (route.kind === "system" && PAGE_MAILBOXES[route.mailbox]) {
    return <Navigate to={PAGE_MAILBOXES[route.mailbox]!} replace />;
  }
  const lens = resolveLens(route, lenses);
  if (!lens) {
    if (phase !== "ready") return <ListSkeleton quiet={phase === "quiet"} />;
    return (
      <Centered
        icon={<SearchX className="size-6" />}
        title={
          route.kind === "label"
            ? "No label by that name"
            : route.kind === "saved"
              ? "No saved search by that name"
              : "No such mailbox"
        }
        body="It may have been renamed or deleted."
        action={
          <Button asChild variant="outline" size="sm">
            <Link to="/m/$mailbox" params={{ mailbox: "inbox" }}>
              Go to Inbox
            </Link>
          </Button>
        }
      />
    );
  }
  return <LensView lens={lens} />;
}

function LensView({ lens }: { lens: MailLens }) {
  const mailbox = useMailboxQuery(lens);
  const groups = useProjectedGroups(mailbox.data?.mailbox.groups, lens.identity);
  const listMode = useUiPrefs((s) => s.listMode);
  const setListMode = useUiPrefs((s) => s.setListMode);
  const account = useUiPrefs((s) => s.accountScope);
  const counts = mailbox.data?.mailbox.counts;

  return (
    <ListWithReader
      basePath={lens.path}
      title={lens.label}
      meta={counts ? countLine(counts.total ?? 0) : null}
      groups={groups}
      scopeKey={`${lens.key}|${account ?? "all"}|${listMode}`}
      status={mailbox}
      hasMore={mailbox.hasNextPage}
      loadingMore={mailbox.isFetchingNextPage}
      onLoadMore={() => void mailbox.fetchNextPage()}
      queueLabel={lens.section === "labels" ? lens.labelName : undefined}
      // Triage lists get gist lines: the inbox and label queues, not Sent,
      // Trash or Spam, where the model would be spent on nothing to decide.
      rowGists={lens.key === "inbox" || lens.section === "labels"}
      // Inbox is the everything view: each row names the mode it went to.
      modeChips={lens.key === "inbox"}
      empty={<EmptyLens lens={lens} />}
      // One persistent control: which grouping is on. The list refreshes
      // itself from daemon events; Sync now lives in the status bar and the
      // palette (docs/web-app-controls.md).
      actions={
        <>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={
                  listMode === "threads" ? "Show single messages" : "Group into conversations"
                }
                onClick={() => setListMode(listMode === "threads" ? "messages" : "threads")}
              >
                {listMode === "threads" ? (
                  <SquareStack className="size-4" />
                ) : (
                  <Rows3 className="size-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {listMode === "threads" ? "Showing conversations" : "Showing single messages"}
            </TooltipContent>
          </Tooltip>
        </>
      }
    />
  );
}

/**
 * The daemon counts messages, not conversations, so say so. No unread
 * count: a pile's size is not work (the desk counts work).
 */
function countLine(total: number): string {
  return total === 0 ? "" : plural(total, "message");
}

function EmptyLens({ lens }: { lens: MailLens }) {
  const sync = useConnectionStore((s) => s.syncProgress);
  const lastEventAt = useConnectionStore((s) => s.lastEventAt);
  if (sync) {
    return (
      <Centered
        icon={<RefreshCw className="size-6 animate-spin" />}
        title="Syncing your mail"
        body={
          sync.total > 0
            ? `${sync.current} of ${plural(sync.total, "message")} so far`
            : "Fetching the first messages"
        }
      />
    );
  }
  if (lens.key === "inbox") {
    return (
      <Centered
        icon={<Inbox className="size-6" />}
        title="Inbox zero"
        body={
          lastEventAt
            ? `Nothing waiting. Last update ${formatRelative(new Date(lastEventAt))}.`
            : "Nothing waiting on you."
        }
        action={
          <Button variant="outline" size="sm" onClick={() => void syncNow()}>
            <RefreshCw className="size-3.5" /> Check for mail
          </Button>
        }
      />
    );
  }
  return (
    <Centered
      icon={<Inbox className="size-6" />}
      title={`Nothing in ${lens.label}`}
      body={
        lens.section === "saved"
          ? "No conversations match this saved search right now."
          : "No conversations here yet."
      }
    />
  );
}
