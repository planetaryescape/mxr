/*
 * The freshness indicator: "Latest mail 5m ago · synced 1m ago" in the
 * status bar on every page, so checking that new mail is reaching you
 * never needs a trip to the inbox. A failing or stale sync replaces the
 * sync words with a warning that links to the sync details. Clicking the
 * mail time opens the last arrivals and where each one went.
 *
 * On a phone it is just "5m ago" and a dot coloured by sync health, with
 * the same popover on tap.
 */

import { Link } from "@tanstack/react-router";
import { AlertTriangle } from "lucide-react";
import { useState } from "react";

import { useSignInAgain } from "./AccountSyncHealth";
import { useFreshnessQuery } from "./api";
import {
  ago,
  effectiveHealth,
  healthTone,
  isCalm,
  latestMail,
  senderName,
  syncLine,
  wentTo,
  worstAccount,
  needsSignIn,
  type AccountFreshness,
  type Arrival,
  type Freshness,
  type SyncHealth,
} from "./copy";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatLongDate, formatTime, formatWhen, startOfDay } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";
import { cn } from "@/lib/utils";

const DOT_TONE = {
  ok: "bg-success",
  warn: "bg-warning",
  bad: "bg-destructive",
} as const;

/** The scope's sync health and words at the clock's time. */
interface SyncState {
  health: SyncHealth;
  line: string;
  /** The account the line is about: where its details live. */
  account?: AccountFreshness;
}

function useSyncState(data: Freshness | undefined): SyncState {
  // Which account is worst, and its words, change together with its health,
  // so this one label drives the re-render and the rest is read at render.
  useClockLabel((now) => {
    const worst = data ? worstAccount(data, now) : undefined;
    return worst && data
      ? `${worst.account_id} ${syncLine(worst, data.stale_after_secs, now)}`
      : "";
  });
  const now = new Date();
  const account = data ? worstAccount(data, now) : undefined;
  if (!account || !data) return { health: "ok", line: "" };
  return {
    account,
    health: effectiveHealth(account, data.stale_after_secs, now),
    line: syncLine(account, data.stale_after_secs, now),
  };
}

/** The account's own page holds its sync details; Diagnostics when unknown. */
function syncDetailsLink(sync: SyncState) {
  return sync.account
    ? ({ to: "/accounts/$key", params: { key: sync.account.account_id } } as const)
    : ({ to: "/diagnostics" } as const);
}

/** "9:42 AM" today, "Mon, Oct 5, 2026, 9:42 AM" before. */
function absolute(value: string): string {
  const date = new Date(value);
  return startOfDay(date) === startOfDay(new Date()) ? formatTime(date) : formatLongDate(date);
}

export function FreshnessIndicator({ compact = false }: { compact?: boolean }) {
  const { data } = useFreshnessQuery();
  const [open, setOpen] = useState(false);
  const newest = data?.newest_message_at ?? null;
  const latest = useClockLabel((now) =>
    compact ? (newest ? ago(newest, now) : "no mail") : latestMail(newest, now),
  );
  const sync = useSyncState(data);
  if (!data) return null;
  const calm = isCalm(sync.health);

  const trigger = (
    <PopoverTrigger asChild>
      <button
        type="button"
        className={cn(
          "inline-flex shrink-0 items-center gap-1.5 rounded px-1 hover:bg-muted hover:text-foreground",
          compact && "h-8 font-mono text-2xs text-muted-foreground",
        )}
        aria-label={
          compact
            ? `${latestMail(newest, new Date())}, ${calm ? sync.line : `warning: ${sync.line}`}. Show the last arrivals`
            : undefined
        }
        data-testid="freshness-trigger"
      >
        <span
          aria-hidden
          data-tone={healthTone(sync.health)}
          className={cn("size-1.5 shrink-0 rounded-full", DOT_TONE[healthTone(sync.health)])}
        />
        <span>{latest}</span>
      </button>
    </PopoverTrigger>
  );

  return (
    <span
      className="inline-flex min-w-0 max-w-[60vw] shrink-0 items-center gap-1.5"
      data-testid="freshness"
    >
      <Popover open={open} onOpenChange={setOpen}>
        {newest ? (
          <Tooltip>
            <TooltipTrigger asChild>{trigger}</TooltipTrigger>
            <TooltipContent>{absolute(newest)}</TooltipContent>
          </Tooltip>
        ) : (
          trigger
        )}
        <PopoverContent
          align={compact ? "end" : "start"}
          side={compact ? "bottom" : "top"}
          className="w-[min(22rem,calc(100vw-2rem))] p-0"
          aria-label="Last arrivals"
        >
          <ArrivalsPanel data={data} sync={sync} onNavigate={() => setOpen(false)} />
        </PopoverContent>
      </Popover>
      {compact ? null : (
        <>
          <span aria-hidden className="text-faint">
            ·
          </span>
          {calm ? (
            <span className="truncate">{sync.line}</span>
          ) : (
            <Link
              {...syncDetailsLink(sync)}
              className="inline-flex min-w-0 items-center gap-1 rounded px-1 text-warning hover:bg-muted"
              title="Sync details"
              data-testid="freshness-warning"
            >
              <AlertTriangle aria-hidden className="size-3 shrink-0" />
              <span className="truncate">{sync.line}</span>
            </Link>
          )}
        </>
      )}
    </span>
  );
}

function ArrivalsPanel({
  data,
  sync,
  onNavigate,
}: {
  data: Freshness;
  sync: SyncState;
  onNavigate: () => void;
}) {
  const calm = isCalm(sync.health);
  const signInAgain = useSignInAgain();
  return (
    <div className="font-sans text-[12.5px]">
      <p className="border-b border-border px-3 py-2 text-2xs font-medium text-muted-foreground">
        Last arrivals
      </p>
      {data.arrivals.length === 0 ? (
        <p className="px-3 py-3 text-muted-foreground">No mail yet.</p>
      ) : (
        <ul className="py-1">
          {data.arrivals.map((arrival) => (
            <li key={arrival.message_id}>
              <ArrivalRow arrival={arrival} onOpen={onNavigate} />
            </li>
          ))}
        </ul>
      )}
      <div className="space-y-1 border-t border-border px-3 py-2 text-2xs text-muted-foreground">
        {data.accounts.length > 1 ? (
          <ul className="space-y-0.5">
            {data.accounts.map((account) => (
              <AccountLine
                key={account.account_id}
                account={account}
                staleAfterSecs={data.stale_after_secs}
              />
            ))}
          </ul>
        ) : null}
        <p className="flex items-center justify-between gap-3">
          <span className={cn("min-w-0", !calm && "text-warning")}>
            {calm ? `Sync: ${sync.line}` : sync.line}
          </span>
          {sync.account && needsSignIn(sync.account) ? (
            <button
              type="button"
              onClick={() => {
                onNavigate();
                if (sync.account) signInAgain(sync.account.account_id);
              }}
              className="shrink-0 font-medium text-foreground underline-offset-2 hover:underline"
            >
              Sign in again
            </button>
          ) : (
            <Link
              {...syncDetailsLink(sync)}
              onClick={onNavigate}
              className="shrink-0 underline-offset-2 hover:text-foreground hover:underline"
            >
              Sync details
            </Link>
          )}
        </p>
      </div>
    </div>
  );
}

function AccountLine({
  account,
  staleAfterSecs,
}: {
  account: AccountFreshness;
  staleAfterSecs: number;
}) {
  const line = useClockLabel(
    (now) =>
      `${account.account_name}: ${latestMail(account.newest_message_at, now)}, ${syncLine(account, staleAfterSecs, now)}`,
  );
  return <li>{line}</li>;
}

function ArrivalRow({ arrival, onOpen }: { arrival: Arrival; onOpen: () => void }) {
  const received = new Date(arrival.received_at);
  const when =
    startOfDay(received) === startOfDay(new Date()) ? formatTime(received) : formatWhen(received);
  return (
    <Link
      to="/m/$mailbox/$threadId"
      params={{ mailbox: "inbox", threadId: arrival.thread_id }}
      search={{ message: arrival.message_id }}
      onClick={onOpen}
      className="grid grid-cols-[4.5rem_minmax(0,1fr)] gap-x-2 px-3 py-1.5 hover:bg-muted focus-visible:bg-muted"
      data-testid="freshness-arrival"
    >
      <time dateTime={arrival.received_at} className="font-mono text-2xs text-muted-foreground">
        {when}
      </time>
      <span className="truncate text-foreground">{senderName(arrival)}</span>
      <span className="col-start-2 truncate text-2xs text-muted-foreground">
        {wentTo(arrival)}
        {arrival.subject ? ` · ${arrival.subject}` : ""}
      </span>
    </Link>
  );
}
