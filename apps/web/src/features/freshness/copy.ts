import type { components } from "@/api/generated";
import { formatTime } from "@/lib/format";

type Schemas = components["schemas"];
export type Freshness = Schemas["FreshnessData"];
export type AccountFreshness = Schemas["AccountFreshnessData"];
export type Arrival = Schemas["ArrivalData"];
export type SyncHealth = Schemas["SyncHealthData"];

const SEVERITY: SyncHealth[] = ["ok", "syncing", "never", "stale", "paused", "failing"];

export function ago(at: string | Date, now: Date): string {
  const seconds = Math.max(0, Math.floor((now.getTime() - new Date(at).getTime()) / 1000));
  if (seconds < 60) return "just now";
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h ago`;
  return `${Math.floor(seconds / 86_400)}d ago`;
}

export function latestMail(newest: string | null | undefined, now: Date): string {
  return newest ? `Latest mail ${ago(newest, now)}` : "No mail yet";
}

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

export function isCalm(health: SyncHealth): boolean {
  return health === "ok" || health === "syncing";
}

export function healthTone(health: SyncHealth): "ok" | "warn" | "bad" {
  if (isCalm(health)) return "ok";
  return health === "failing" ? "bad" : "warn";
}

function retrying(account: AccountFreshness, now: Date): string | null {
  const retryAt = account.last_sync_error?.retry_at;
  if (account.sync_in_progress) return "syncing now";
  if (retryAt && new Date(retryAt) <= now) return "retrying now";
  return retryAt ? `retrying ${formatTime(new Date(retryAt))}` : null;
}

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
      const retry = retrying(account, now);
      return retry ? `${label} paused: rate limited, ${retry}` : `${label} paused: rate limited`;
    }
    case "failing": {
      const kind = account.last_sync_error?.kind ?? "unknown";
      if (kind === "auth") return `${label} needs you to sign in again`;
      if (kind === "offline") {
        const retry = retrying(account, now);
        if (retry) return `Can't reach ${label}, ${retry}`;
        return lastOk ? `Can't reach ${label}, last sync ${lastOk}` : `Can't reach ${label}`;
      }
      const what = kind === "rate_limited" ? "rate limited" : "sync failing";
      const retry = retrying(account, now);
      if (retry) return `${label} ${what}, ${retry}`;
      return lastOk ? `${label} ${what}, last sync ${lastOk}` : `${label} ${what}`;
    }
  }
}

export function wentTo(arrival: Arrival): string {
  const mode = arrival.modes[0];
  if (mode) {
    const destination = mode.tag ? `${mode.name} · ${mode.tag}` : mode.name;
    return arrival.in_inbox ? `→ ${destination}` : `→ out of the inbox · ${destination}`;
  }
  return arrival.in_inbox ? "→ Inbox" : "→ out of the inbox";
}

export function senderName(arrival: Arrival): string {
  return arrival.from.name?.trim() || arrival.from.email;
}

export function shortHealth(account: AccountFreshness, staleAfterSecs: number, now: Date): string {
  const lastOk = account.last_sync_ok_at ? ago(account.last_sync_ok_at, now) : null;
  const retry = retrying(account, now);
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
      return retry ? `Rate limited, ${retry}` : "Rate limited";
    case "failing":
      if (account.last_sync_error?.kind === "auth") return "Signed out";
      return retry ? `Can't sync, ${retry}` : "Can't sync";
  }
}

export function plainReason(account: AccountFreshness, now: Date): string | null {
  const error = account.last_sync_error;
  if (!error) return null;
  const retry = error.retry_at && new Date(error.retry_at) > now ? new Date(error.retry_at) : null;
  const next = retry ? ` Trying again at ${formatTime(retry)}.` : "";
  switch (error.kind) {
    case "rate_limited":
      return `${account.label} asked mxr to slow down.${next}`;
    case "auth":
      return `${account.label} needs you to sign in again.`;
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

export function needsSignIn(account: AccountFreshness): boolean {
  return account.health === "failing" && account.last_sync_error?.kind === "auth";
}
