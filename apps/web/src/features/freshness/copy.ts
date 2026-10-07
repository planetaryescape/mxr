/*
 * Freshness words: when the newest mail came in and how sync is doing.
 * The same rules and words as the daemon's `freshness_copy`
 * (crates/protocol/src/types/freshness.rs), which the TUI and CLI use;
 * here so the ages tick in the browser between fetches.
 */

import type { components } from "@/api/generated";
import { formatTime } from "@/lib/format";

type Schemas = components["schemas"];
export type Freshness = Schemas["FreshnessData"];
export type AccountFreshness = Schemas["AccountFreshnessData"];
export type Arrival = Schemas["ArrivalData"];
export type SyncHealth = Schemas["SyncHealthData"];

/** Least worrying first: the "All accounts" scope shows the last one. */
const SEVERITY: SyncHealth[] = ["ok", "syncing", "never", "stale", "paused", "failing"];

/** "just now", "5m ago", "2h ago", "3d ago". */
export function ago(at: string | Date, now: Date): string {
  const seconds = Math.max(0, Math.floor((now.getTime() - new Date(at).getTime()) / 1000));
  if (seconds < 60) return "just now";
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h ago`;
  return `${Math.floor(seconds / 86_400)}d ago`;
}

/** "Latest mail 5m ago", or "No mail yet". */
export function latestMail(newest: string | null | undefined, now: Date): string {
  return newest ? `Latest mail ${ago(newest, now)}` : "No mail yet";
}

/** `health` at `now`: a good sync that has aged past the limit is stale. */
export function effectiveHealth(
  account: AccountFreshness,
  staleAfterSecs: number,
  now: Date,
): SyncHealth {
  const lastOk = account.last_sync_ok_at;
  if (account.health === "ok" && lastOk) {
    const age = (now.getTime() - new Date(lastOk).getTime()) / 1000;
    if (age > staleAfterSecs) return "stale";
  }
  return account.health;
}

/** The account in the worst sync state at `now`: the scope's sync line. */
export function worstAccount(data: Freshness, now: Date): AccountFreshness | undefined {
  let worst: AccountFreshness | undefined;
  let worstRank = -1;
  for (const account of data.accounts) {
    const rank = SEVERITY.indexOf(effectiveHealth(account, data.stale_after_secs, now));
    if (rank > worstRank) {
      worst = account;
      worstRank = rank;
    }
  }
  return worst;
}

/** Calm states need no attention; the rest are a warning. */
export function isCalm(health: SyncHealth): boolean {
  return health === "ok" || health === "syncing";
}

/** The dot's colour on a phone, where the words don't fit. */
export function healthTone(health: SyncHealth): "ok" | "warn" | "bad" {
  if (isCalm(health)) return "ok";
  return health === "failing" ? "bad" : "warn";
}

/**
 * One account's sync line: "synced 2m ago", "Gmail paused: rate limited,
 * retrying 9:50 AM", "Last sync 2h ago".
 */
export function syncLine(account: AccountFreshness, staleAfterSecs: number, now: Date): string {
  const lastOk = account.last_sync_ok_at ? ago(account.last_sync_ok_at, now) : null;
  const label = account.label;
  switch (effectiveHealth(account, staleAfterSecs, now)) {
    case "ok":
      return lastOk ? `synced ${lastOk}` : "synced";
    case "syncing":
      return "first sync running";
    case "never":
      return "not synced yet";
    case "stale":
      return lastOk ? `Last sync ${lastOk}` : "not synced yet";
    case "paused": {
      const retry = account.last_sync_error?.retry_at;
      if (!retry) return `${label} paused: rate limited`;
      const at = new Date(retry);
      return at > now
        ? `${label} paused: rate limited, retrying ${formatTime(at)}`
        : `${label} paused: rate limited, retrying now`;
    }
    case "failing": {
      const kind = account.last_sync_error?.kind ?? "unknown";
      if (kind === "auth") return `${label} needs you to sign in again`;
      const what =
        kind === "rate_limited"
          ? "rate limited"
          : kind === "offline"
            ? "unreachable"
            : "sync failing";
      const retry = account.last_sync_error?.retry_at;
      if (retry && new Date(retry) > now)
        return `${label} ${what}, retrying ${formatTime(new Date(retry))}`;
      return lastOk ? `${label} ${what}, last sync ${lastOk}` : `${label} ${what}`;
    }
  }
}

/** Where an arrival went: "→ Updates · automated", "→ Inbox". */
export function wentTo(arrival: Arrival): string {
  const mode = arrival.modes[0];
  if (mode) return mode.tag ? `→ ${mode.name} · ${mode.tag}` : `→ ${mode.name}`;
  return arrival.in_inbox ? "→ Inbox" : "→ out of the inbox";
}

/** The name to show for a sender. */
export function senderName(arrival: Arrival): string {
  return arrival.from.name?.trim() || arrival.from.email;
}

/**
 * An account's health in a few words, for a row or a menu: "Synced 2m
 * ago", "Can't sync, retrying 2:05 PM", "Signed out".
 */
export function shortHealth(account: AccountFreshness, staleAfterSecs: number, now: Date): string {
  const lastOk = account.last_sync_ok_at ? ago(account.last_sync_ok_at, now) : null;
  const retry = account.last_sync_error?.retry_at;
  const retrying =
    retry && new Date(retry) > now ? `retrying ${formatTime(new Date(retry))}` : null;
  switch (effectiveHealth(account, staleAfterSecs, now)) {
    case "ok":
      return lastOk ? `Synced ${lastOk}` : "Synced";
    case "syncing":
      return "First sync running";
    case "never":
      return "Not synced yet";
    case "stale":
      return lastOk ? `Last sync ${lastOk}` : "Not synced yet";
    case "paused":
      return retrying ? `Rate limited, ${retrying}` : "Rate limited";
    case "failing":
      if (account.last_sync_error?.kind === "auth") return "Signed out";
      return retrying ? `Can't sync, ${retrying}` : "Can't sync";
  }
}

/** Why sync is failing, in plain words, for the account's Sync section. */
export function plainReason(account: AccountFreshness, now: Date): string | null {
  const error = account.last_sync_error;
  if (!error) return null;
  const retry = error.retry_at && new Date(error.retry_at) > now ? new Date(error.retry_at) : null;
  const next = retry ? ` Trying again at ${formatTime(retry)}.` : "";
  switch (error.kind) {
    case "rate_limited":
      return `${account.label} asked mxr to slow down.${next}`;
    case "auth":
      return `${account.label} signed mxr out. Sign in again to keep syncing.`;
    case "offline":
      return `Can't reach ${account.label}.${next}`;
    case "provider":
      return `${account.label} turned the sync down.${next}`;
    case "store":
      return `mxr couldn't save what it synced.${next}`;
    case "unknown":
      return `Sync is failing.${next}`;
  }
}

/** The provider signed mxr out: the fix is to sign in again. */
export function needsSignIn(account: AccountFreshness): boolean {
  return account.health === "failing" && account.last_sync_error?.kind === "auth";
}
