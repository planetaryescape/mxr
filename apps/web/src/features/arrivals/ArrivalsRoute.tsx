import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { PageFrame } from "@/components/PageFrame";
import { Button } from "@/components/ui/button";
import { PageError, PageSkeleton } from "@/components/PageParts";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { formatTime } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";

import { useArrivalList } from "./api";
import { listTitle } from "./arrivalLine";
import { openMovePicker } from "./moves";
import { moveCommands } from "./moveSubjects";
import type { ArrivalBucket, ArrivalItem } from "./types";

/**
 * The emails behind one count of Now's arrivals line, newest first, in
 * the line's own window: exactly as many rows as the count. Each row says
 * where it went and why, and `X` / `P` move it or its sender.
 */
export function ArrivalsRoute({
  bucket,
  since,
  until,
}: {
  bucket?: ArrivalBucket;
  since?: string;
  until?: string;
}) {
  const list = useArrivalList({ bucket, since, until });
  const phase = useDelayedPending(list.isLoading);
  return (
    <section
      aria-labelledby="arrivals-title"
      className="flex min-h-0 flex-1 flex-col bg-background"
    >
      <PageFrame
        titleId="arrivals-title"
        eyebrow={
          <Link to="/now" className="hover:text-text-1 hover:underline">
            Back to Now
          </Link>
        }
        title={list.data ? listTitle(list.data.total, bucket, list.data.since) : "Arrivals"}
        purpose={
          <>
            Newest first. <KeyChip>X</KeyChip> moves an email, <KeyChip>P</KeyChip> everything from
            its sender.
          </>
        }
      />
      {phase !== "ready" ? (
        <PageSkeleton framed lead="avatar" shown={phase === "skeleton"} />
      ) : list.isError ? (
        <PageError
          framed
          title="Couldn't load these emails"
          error={list.error}
          onRetry={() => void list.refetch()}
        />
      ) : list.data && list.data.items.length > 0 ? (
        <ArrivalRows items={list.data.items} total={list.data.total} />
      ) : (
        <p className="px-5 py-6 text-sm text-muted-foreground">None arrived here in this window.</p>
      )}
    </section>
  );
}

function ArrivalRows({ items, total }: { items: ArrivalItem[]; total: number }) {
  const navigate = useNavigate();
  const [cursor, setCursor] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const at = Math.min(cursor, items.length - 1);
  useEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>(`[data-index="${at}"]`);
    if (row && listRef.current?.contains(document.activeElement)) row.focus();
  }, [at]);

  // The page's keys (j, k, Enter, X, P) come from the key definition, so
  // the global keys keep working beside them and `u` undoes an `X` move.
  useShortcutScope("arrivals");
  useScopeController("arrivals", {
    down: () => setCursor(Math.min(items.length - 1, at + 1)),
    up: () => setCursor(Math.max(0, at - 1)),
    open: () => {
      const item = items[at];
      if (item) void navigate(threadTarget(item));
    },
    ...moveCommands(() => {
      const item = items[at];
      return item ? subjectOf(item) : null;
    }),
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-6">
      <ul ref={listRef} aria-label="Emails" className="grid grid-cols-[minmax(0,1fr)]">
        {items.map((item, index) => (
          <li
            key={item.message_id}
            data-index={index}
            data-testid="arrival-row"
            tabIndex={0}
            aria-current={index === at ? "true" : undefined}
            onFocus={() => setCursor(index)}
            className={cn(
              "grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-3 border-b border-border/60 px-5 py-2 outline-none focus-visible:bg-accent/60",
              index === at && "bg-accent/40",
            )}
          >
            <Link {...threadTarget(item)} className="min-w-0">
              <span className={cn("block truncate text-sm", item.unread && "font-semibold")}>
                {item.sender_name || item.sender_email}
              </span>
              <span className="block truncate text-sm text-foreground/90">
                {item.subject || "(no subject)"}
              </span>
              <span className="block truncate text-xs text-muted-foreground">{item.chip}</span>
            </Link>
            <span className="flex items-center gap-1 text-xs text-muted-foreground">
              <time dateTime={item.first_seen_at}>{formatTime(new Date(item.first_seen_at))}</time>
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2 text-xs"
                aria-label={`Move "${item.subject || "(no subject)"}" to another mode`}
                onClick={() => openMovePicker(subjectOf(item))}
              >
                Move
              </Button>
            </span>
          </li>
        ))}
      </ul>
      {total > items.length ? (
        <p className="px-5 pt-3 text-xs text-muted-foreground">
          The newest {items.length} of {total}.
        </p>
      ) : null}
    </div>
  );
}

function threadTarget(item: ArrivalItem) {
  // All mail holds an arrival wherever it went since. The account tells apart
  // two accounts that hold the same thread id.
  return {
    to: `/m/archive/${encodeURIComponent(item.thread_id)}`,
    search: { account: item.account_id },
  } as { to: string };
}

function subjectOf(item: ArrivalItem) {
  return {
    messageId: item.message_id,
    label: item.subject || "(no subject)",
    senderLabel: item.sender_name || item.sender_email,
  };
}
