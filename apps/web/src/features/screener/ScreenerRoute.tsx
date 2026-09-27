import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Shield, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import {
  clearScreenerDecision,
  fetchScreenerDecisions,
  fetchScreenerQueue,
  setScreenerDecision,
  type ScreenerDisposition,
  type ScreenerEntry,
} from "./api";
import { KeyChip } from "@/components/KeyChip";
import { Page, PageTabs } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList, RuledRow } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { fetchAccounts } from "@/features/accounts/api";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { formatListDate, formatRelative, plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";

type Tab = "queue" | "decisions";

/** Button order and key hints mirror `screenerActions` (a, d, f, p). */
const DECISIONS: { disposition: ScreenerDisposition; label: string; key: string }[] = [
  { disposition: "allow", label: "Allow", key: "a" },
  { disposition: "deny", label: "Deny", key: "d" },
  { disposition: "feed", label: "Feed", key: "f" },
  { disposition: "paper_trail", label: "Paper trail", key: "p" },
];

const DISPOSITION_LABELS: Record<ScreenerDisposition, string> = {
  allow: "Allowed",
  deny: "Denied",
  feed: "Feed",
  paper_trail: "Paper trail",
  unknown: "Unknown",
};

export function ScreenerRoute() {
  const [tab, setTab] = useState<Tab>("queue");
  const [accountId, setAccountId] = useState<string | null>(null);
  // The app's account scope, so the desk's screener count and this queue
  // are the same account's.
  const scope = useUiPrefs((s) => s.accountScope);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountList = accounts.data?.accounts ?? [];
  // Default to the scoped account, else the first, until the user picks one.
  const activeAccountId = accountId ?? scope ?? accountList[0]?.account_id ?? null;
  const account = accountList.find((item) => item.account_id === activeAccountId);

  const accountPicker =
    accountList.length > 1 ? (
      <Select value={activeAccountId ?? undefined} onValueChange={setAccountId}>
        <SelectTrigger className="h-8 w-[220px] text-xs" aria-label="Screener account">
          <SelectValue placeholder="Select account" />
        </SelectTrigger>
        <SelectContent>
          {accountList.map((item) => (
            <SelectItem key={item.account_id} value={item.account_id} className="text-xs">
              {item.email}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    ) : account ? (
      <span className="font-mono text-2xs text-muted-foreground">{account.email}</span>
    ) : null;

  return (
    <Page
      title="Screener"
      description={
        accountList.length > 1
          ? "First-time senders wait here until you decide. Each account has its own queue."
          : "First-time senders wait here until you decide."
      }
      actions={accountPicker}
      tabs={
        <PageTabs
          label="Screener views"
          value={tab}
          onChange={setTab}
          tabs={[
            { id: "queue", label: "Queue" },
            { id: "decisions", label: "Decisions" },
          ]}
        />
      }
    >
      {accounts.isPending ? (
        <PageSkeleton label="Loading accounts" />
      ) : accounts.isError ? (
        <PageError
          title="Accounts unavailable"
          error={accounts.error}
          onRetry={() => void accounts.refetch()}
        />
      ) : !account ? (
        <PageEmpty
          icon={<Users className="size-5" />}
          title="No account to screen"
          body="Connect an account first; its unknown senders queue up here."
        />
      ) : tab === "queue" ? (
        <ScreenerQueue key={account.account_id} accountId={account.account_id} />
      ) : (
        <ScreenerDecisions accountId={account.account_id} />
      )}
    </Page>
  );
}

function ScreenerQueue({ accountId }: { accountId: string }) {
  const qc = useQueryClient();
  const [focused, setFocused] = useState(0);
  const queue = useQuery({
    queryKey: ["screener", accountId],
    queryFn: () => fetchScreenerQueue(accountId),
  });
  const decide = useMutation({
    mutationFn: ({
      entry,
      disposition,
    }: {
      entry: ScreenerEntry;
      disposition: ScreenerDisposition;
    }) => setScreenerDecision({ accountId, senderEmail: entry.sender_email, disposition }),
    onSuccess: (_result, { entry, disposition }) => {
      toast.success(
        `${DISPOSITION_LABELS[disposition]}: ${entry.display_name || entry.sender_email}`,
      );
      void qc.invalidateQueries({ queryKey: ["screener", accountId] });
      void qc.invalidateQueries({ queryKey: ["screener-decisions", accountId] });
    },
    onError: (error, { entry }) =>
      toast.error(`Could not screen ${entry.sender_email}`, { description: error.message }),
  });
  const rows = queue.data?.entries ?? [];

  useEffect(() => {
    if (focused >= rows.length) setFocused(Math.max(0, rows.length - 1));
  }, [focused, rows.length]);

  const decideFocused = (disposition: ScreenerDisposition) => {
    const entry = rows[focused];
    if (!entry || decide.isPending) return;
    decide.mutate({ entry, disposition });
  };

  // The keys live in the action registry (features/screener/actions.ts);
  // this view supplies what they do while the queue is on screen.
  useShortcutScope("screener", rows.length > 0);
  useScopeController("screener", {
    allow: () => decideFocused("allow"),
    deny: () => decideFocused("deny"),
    feed: () => decideFocused("feed"),
    paperTrail: () => decideFocused("paper_trail"),
    down: () => setFocused((current) => Math.min(rows.length - 1, current + 1)),
    up: () => setFocused((current) => Math.max(0, current - 1)),
  });

  if (queue.isPending) return <PageSkeleton label="Loading screener queue" />;
  if (queue.isError)
    return (
      <PageError
        title="Screener queue unavailable"
        error={queue.error}
        onRetry={() => void queue.refetch()}
      />
    );
  if (rows.length === 0)
    return (
      <PageEmpty
        icon={<Shield className="size-5" />}
        title="Queue empty"
        body="No unknown senders are waiting. New ones land here before they reach your inbox."
      />
    );

  return (
    <>
      <p className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-muted-foreground">
        <span>{plural(rows.length, "sender")} waiting.</span>
        <span className="inline-flex items-center gap-1">
          <KeyChip>j</KeyChip>
          <KeyChip>k</KeyChip> to move, then the key on each button
        </span>
      </p>
      <RuledList label="Screener queue">
        {rows.map((entry, index) => (
          <RuledRow
            key={entry.sender_email}
            current={index === focused}
            title={entry.display_name || entry.sender_email}
            meta={`${entry.sender_email} · ${plural(entry.message_count, "message")} · ${entry.latest_subject}`}
            aside={formatListDate(entry.latest_at)}
            onOpen={() => setFocused(index)}
            openLabel={`Focus ${entry.sender_email}`}
            actions={DECISIONS.map(({ disposition, label, key }) => (
              <Button
                key={disposition}
                variant={disposition === "deny" ? "destructive" : "outline"}
                size="xs"
                disabled={decide.isPending}
                onClick={() => decide.mutate({ entry, disposition })}
                aria-keyshortcuts={index === focused ? key : undefined}
              >
                {label}
                {index === focused ? <KeyChip className="ml-0.5 h-4 px-1">{key}</KeyChip> : null}
              </Button>
            ))}
          />
        ))}
      </RuledList>
    </>
  );
}

function ScreenerDecisions({ accountId }: { accountId: string }) {
  const qc = useQueryClient();
  const decisions = useQuery({
    queryKey: ["screener-decisions", accountId],
    queryFn: () => fetchScreenerDecisions(accountId),
  });
  const clear = useMutation({
    mutationFn: (senderEmail: string) => clearScreenerDecision({ accountId, senderEmail }),
    onSuccess: (_result, senderEmail) => {
      toast.success(`Cleared decision for ${senderEmail}`);
      void qc.invalidateQueries({ queryKey: ["screener-decisions", accountId] });
      void qc.invalidateQueries({ queryKey: ["screener", accountId] });
    },
    onError: (error) => toast.error("Clear failed", { description: error.message }),
  });
  const rows = decisions.data?.decisions ?? [];

  if (decisions.isPending) return <PageSkeleton label="Loading decisions" />;
  if (decisions.isError)
    return (
      <PageError
        title="Decisions unavailable"
        error={decisions.error}
        onRetry={() => void decisions.refetch()}
      />
    );
  if (rows.length === 0)
    return (
      <PageEmpty
        icon={<Shield className="size-5" />}
        title="No decisions yet"
        body="Senders you allow, deny, feed or paper-trail are listed here, so you can undo a call."
      />
    );

  return (
    <RuledList label="Screener decisions">
      {rows.map((decision) => (
        <RuledRow
          key={decision.sender_email}
          title={decision.sender_email}
          meta={`${DISPOSITION_LABELS[decision.disposition] ?? decision.disposition}${decision.route_label ? ` to ${decision.route_label}` : ""} · ${formatRelative(decision.decided_at)}`}
          actions={
            <Button
              variant="ghost"
              size="xs"
              disabled={clear.isPending}
              onClick={() => clear.mutate(decision.sender_email)}
            >
              Clear
            </Button>
          }
        />
      ))}
    </RuledList>
  );
}
