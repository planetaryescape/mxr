import { Link, useNavigate } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";

import { useArrivalList } from "./api";
import { listTitle } from "./arrivalLine";
import { answerPendingSenderAsk, openMovePicker } from "./moves";
import type { ArrivalBucket, ArrivalItem } from "./types";

/**
 * The emails behind one count of Now's arrivals line, newest first, in
 * the line's own window: exactly as many rows as the count. Each row says
 * where it went and why, and `X` / `K` move it or its sender.
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
      <header className="shrink-0 border-b border-border px-5 pb-3 pt-4">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h1 id="arrivals-title" className="text-[17px] font-semibold tracking-tight">
            {list.data ? listTitle(list.data.total, bucket, list.data.since) : "Arrivals"}
          </h1>
          <Link to="/now" className="text-[12.5px] text-muted-foreground hover:underline">
            Back to Now
          </Link>
        </div>
        <p className="mt-1 text-[12.5px] text-muted-foreground">
          Newest first. <KeyChip>X</KeyChip> moves an email, <KeyChip>K</KeyChip> everything from
          its sender.
        </p>
      </header>
      {phase !== "ready" ? (
        <ListSkeleton quiet={phase === "quiet"} />
      ) : list.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load these emails"
          body={list.error.message}
          action={
            <Button size="sm" onClick={() => void list.refetch()}>
              Try again
            </Button>
          }
        />
      ) : list.data && list.data.items.length > 0 ? (
        <ArrivalRows items={list.data.items} total={list.data.total} />
      ) : (
        <p className="px-5 py-6 text-[13px] text-muted-foreground">
          None arrived here in this window.
        </p>
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

  const onKeyDown = (event: KeyboardEvent<HTMLLIElement>, item: ArrivalItem, index: number) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const act = (run: () => void) => {
      event.preventDefault();
      run();
    };
    switch (event.key) {
      case "j":
      case "ArrowDown":
        return act(() => setCursor(Math.min(items.length - 1, index + 1)));
      case "k":
      case "ArrowUp":
        return act(() => setCursor(Math.max(0, index - 1)));
      case "Enter":
      case "o":
        return act(() => void navigate({ to: threadPath(item) }));
      case "X":
        return act(() => openMovePicker(subjectOf(item)));
      case "K":
        return act(() => {
          if (!answerPendingSenderAsk()) openMovePicker(subjectOf(item), true);
        });
    }
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-6">
      <ul ref={listRef} aria-label="Emails" className="grid grid-cols-[minmax(0,1fr)]">
        {items.map((item, index) => (
          // A row owns its keys: j, k, Enter, X and K act on it.
          <li
            key={item.message_id}
            data-index={index}
            data-testid="arrival-row"
            data-keys-owner
            tabIndex={0}
            aria-current={index === at ? "true" : undefined}
            onFocus={() => setCursor(index)}
            onKeyDown={(event) => onKeyDown(event, item, index)}
            className={cn(
              "grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-3 border-b border-border/60 px-5 py-2 outline-none focus-visible:bg-accent/60",
              index === at && "bg-accent/40",
            )}
          >
            <Link to={threadPath(item)} className="min-w-0">
              <span className={cn("block truncate text-[13px]", item.unread && "font-semibold")}>
                {item.sender_name || item.sender_email}
              </span>
              <span className="block truncate text-[13px] text-foreground/90">
                {item.subject || "(no subject)"}
              </span>
              <span className="block truncate text-[12px] text-muted-foreground">{item.chip}</span>
            </Link>
            <span className="flex items-center gap-1 text-[12px] text-muted-foreground">
              <time dateTime={item.first_seen_at}>{formatTime(new Date(item.first_seen_at))}</time>
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2 text-[12px]"
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
        <p className="px-5 pt-3 text-[12px] text-muted-foreground">
          The newest {items.length} of {total}.
        </p>
      ) : null}
    </div>
  );
}

function threadPath(item: ArrivalItem): string {
  // All mail holds an arrival wherever it went since.
  return `/m/archive/${encodeURIComponent(item.thread_id)}`;
}

function subjectOf(item: ArrivalItem) {
  return {
    messageId: item.message_id,
    label: item.subject || "(no subject)",
    senderLabel: item.sender_name || item.sender_email,
  };
}
