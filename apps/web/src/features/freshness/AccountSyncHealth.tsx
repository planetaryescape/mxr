import { useNavigate } from "@tanstack/react-router";
import { KeyRound } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";

import { useAllAccountsFreshness } from "./api";
import {
  ago,
  effectiveHealth,
  healthTone,
  needsSignIn,
  plainReason,
  shortHealth,
  type AccountFreshness,
  type SyncHealth,
} from "./copy";
import { Button } from "@/components/ui/button";
import { requestAccountReauth } from "@/features/accounts/reauthRequest";
import { formatTime } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";
import { cn } from "@/lib/utils";
import { StatusDot } from "@/components/StatusDot";

const DOT_TONE = {
  ok: "bg-success",
  warn: "bg-warning",
  bad: "bg-destructive",
} as const;

const TEXT_TONE = {
  ok: "text-muted-foreground",
  warn: "text-warning",
  bad: "text-destructive",
} as const;

function useAccount(accountId: string) {
  const { data } = useAllAccountsFreshness();
  return { account: data?.accounts.find((item) => item.account_id === accountId), data };
}

function useHealth(account: AccountFreshness | undefined, staleAfterSecs: number) {
  const words = useClockLabel((now) => (account ? shortHealth(account, staleAfterSecs, now) : ""));
  const health: SyncHealth = account ? effectiveHealth(account, staleAfterSecs, new Date()) : "ok";
  return { health, words };
}

export function SyncHealthDot({ accountId, className }: { accountId: string; className?: string }) {
  const { account, data } = useAccount(accountId);
  const { health, words } = useHealth(account, data?.stale_after_secs ?? 0);
  if (!account) return null;
  const tone = healthTone(health);
  return (
    <StatusDot
      role="img"
      aria-hidden={false}
      aria-label={`Sync: ${words}`}
      title={words}
      data-tone={tone}
      className={cn("size-1.5", DOT_TONE[tone], className)}
    />
  );
}

export function SyncHealthWords({ accountId }: { accountId: string }) {
  const { account, data } = useAccount(accountId);
  const { health, words } = useHealth(account, data?.stale_after_secs ?? 0);
  if (!account) return null;
  const tone = healthTone(health);
  return (
    <span
      className={cn("inline-flex items-center gap-1.5 font-mono text-2xs", TEXT_TONE[tone])}
      data-testid="account-sync-health"
      data-tone={tone}
    >
      <StatusDot className={cn("size-1.5", DOT_TONE[tone])} />
      {words}
    </span>
  );
}

export function useSignInAgain(): (accountId: string) => void {
  const navigate = useNavigate();
  return useCallback(
    (accountId: string) => {
      requestAccountReauth(accountId);
      void navigate({ to: "/accounts/$key", params: { key: accountId } });
    },
    [navigate],
  );
}

export function AccountSyncDetails({
  accountId,
  onSignIn,
}: {
  accountId: string;
  onSignIn?: () => void;
}) {
  const { account, data } = useAccount(accountId);
  const { health, words } = useHealth(account, data?.stale_after_secs ?? 0);
  const lastOk = useClockLabel((now) =>
    account?.last_sync_ok_at ? ago(account.last_sync_ok_at, now) : "never",
  );
  const reason = useClockLabel((now) => (account ? (plainReason(account, now) ?? "") : ""));
  if (!account) return <p className="text-sm text-muted-foreground">No sync reading yet.</p>;
  const tone = healthTone(health);
  const retry = account.last_sync_error?.retry_at;
  return (
    <div className="space-y-2 text-sm" data-testid="account-sync-details">
      <p className={cn("flex items-center gap-2", TEXT_TONE[tone])} role="status">
        <StatusDot className={cn("size-1.5", DOT_TONE[tone])} />
        {words}
      </p>
      <dl className="grid grid-cols-[8rem_minmax(0,1fr)] gap-y-1 text-muted-foreground">
        <dt>Last good sync</dt>
        <dd className="text-foreground">{lastOk}</dd>
        {retry ? (
          <>
            <dt>Next try</dt>
            <dd className="text-foreground">{formatTime(new Date(retry))}</dd>
          </>
        ) : null}
      </dl>
      {reason ? <p>{reason}</p> : null}
      {needsSignIn(account) && onSignIn ? (
        <Button size="sm" onClick={onSignIn}>
          <KeyRound className="size-3" />
          Sign in again
        </Button>
      ) : null}
      {account.last_sync_error ? (
        <details className="text-2xs text-muted-foreground">
          <summary className="cursor-pointer">Details</summary>
          <p className="mt-1 font-mono break-words">{account.last_sync_error.message}</p>
        </details>
      ) : null}
    </div>
  );
}

export function useSyncHealthToasts(): void {
  const { data } = useAllAccountsFreshness();
  const signInAgain = useSignInAgain();
  const seen = useRef<Map<string, boolean> | null>(null);
  useEffect(() => {
    if (!data) return;
    const failing = (account: AccountFreshness) =>
      account.health === "failing" || account.health === "paused";
    const previous = seen.current;
    const next = new Map(data.accounts.map((account) => [account.account_id, failing(account)]));
    seen.current = next;
    if (!previous) return;
    for (const account of data.accounts) {
      const was = previous.get(account.account_id);
      const now = failing(account);
      if (was === undefined || was === now) continue;
      const id = `sync-health-${account.account_id}`;
      if (now) {
        toast.warning(`${account.label} can't sync`, {
          id,
          description: plainReason(account, new Date()) ?? undefined,
          action: needsSignIn(account)
            ? { label: "Sign in again", onClick: () => signInAgain(account.account_id) }
            : undefined,
        });
      } else {
        toast.success(`${account.label} is syncing again`, { id });
      }
    }
  }, [data, signInAgain]);
}
