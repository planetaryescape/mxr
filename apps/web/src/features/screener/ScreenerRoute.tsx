import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, Check, Receipt, Rss, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import {
  clearScreenerDecision,
  fetchScreenerDecisions,
  fetchScreenerQueue,
  setScreenerDecision,
  type ScreenerDisposition,
  type ScreenerEntry,
} from "./api";
import { Avatar } from "@/components/Avatar";
import { Page, PageTabs } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList, RuledRow } from "@/components/PageParts";
import { Row, RowVerb, type LampVia } from "@/components/Row";
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
import { formatWhen, formatRelative, plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useScopeController } from "@/lib/keys/controllers";
import { stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { useHeldPending } from "@/hooks/useDelayedPending";

type Tab = "queue" | "decisions";

/** The lit row's choice. The number answers beside the TUI's letter (a, d, f, p). */
const DECISIONS: {
  disposition: ScreenerDisposition;
  label: string;
  key: string;
  Icon: LucideIcon;
}[] = [
  { disposition: "allow", label: "Allow", key: "1", Icon: Check },
  { disposition: "deny", label: "Deny", key: "2", Icon: Ban },
  { disposition: "feed", label: "Feed", key: "3", Icon: Rss },
  { disposition: "paper_trail", label: "Paper trail", key: "4", Icon: Receipt },
];

const DISPOSITION_LABELS: Record<ScreenerDisposition, string> = {
  allow: "Allowed",
  deny: "Denied",
  feed: "Feed",
  paper_trail: "Paper trail",
  unknown: "Unknown",
};

export function ScreenerRoute({ account: linked }: { account?: string } = {}) {
  const [tab, setTab] = useState<Tab>("queue");
  const [accountId, setAccountId] = useState<string | null>(null);
  // The app's account scope, so the desk's screener count and this queue
  // are the same account's.
  const scope = useUiPrefs((s) => s.accountScope);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountsHeld = useHeldPending(accounts.isPending);
  const accountList = accounts.data?.accounts ?? [];
  // The user's pick, else the account a link named, else the app's scope,
  // else the first account.
  const activeAccountId = accountId ?? linked ?? scope ?? accountList[0]?.account_id ?? null;
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
      toolbar={accountPicker}
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
      {accounts.isPending || accountsHeld ? (
        <PageSkeleton label="Loading accounts" />
      ) : accounts.isError ? (
        <PageError
          title="Accounts unavailable"
          error={accounts.error}
          onRetry={() => void accounts.refetch()}
        />
      ) : !account ? (
        <PageEmpty
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
  const via = useRef<LampVia>("key");
  const [allSenders, setAllSenders] = useState(false);
  const queue = useQuery({
    queryKey: ["screener", accountId, allSenders],
    queryFn: () =>
      allSenders ? fetchScreenerQueue(accountId, true) : fetchScreenerQueue(accountId),
  });
  const queueHeld = useHeldPending(queue.isPending);
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
    down: () => {
      via.current = "key";
      setFocused((current) => Math.min(rows.length - 1, current + 1));
    },
    up: () => {
      via.current = "key";
      setFocused((current) => Math.max(0, current - 1));
    },
  });
  usePageKeys(rows.length > 0 ? stripKeys("screener") : []);

  if (queue.isPending || queueHeld) return <PageSkeleton label="Loading screener queue" />;
  if (queue.isError)
    return (
      <PageError
        title="Screener queue unavailable"
        error={queue.error}
        onRetry={() => void queue.refetch()}
      />
    );
  const scope = (
    <Button variant="link" size="xs" onClick={() => setAllSenders((current) => !current)}>
      {allSenders ? "Only new senders" : "Show all senders"}
    </Button>
  );
  if (rows.length === 0)
    return (
      <>
        <PageEmpty
          title="Queue empty"
          body={
            allSenders
              ? "Every sender has a decision."
              : "No new senders are waiting. New ones land here before they reach your inbox."
          }
        />
        <div className="mt-2 flex justify-center">{scope}</div>
      </>
    );

  return (
    <>
      <p className="type-meta mb-3 flex flex-wrap items-center gap-x-3 px-2 text-text-3">
        <span>
          {plural(rows.length, "sender")} {allSenders ? "without a decision." : "waiting."}
        </span>
        {scope}
      </p>
      <ul aria-label="Screener queue" data-mode-pane="" tabIndex={-1} className="outline-none">
        {rows.map((entry, index) => {
          const name = entry.display_name || entry.sender_email;
          return (
            <Row
              key={entry.sender_email}
              accountId={accountId}
              data-testid="screener-row"
              lit={index === focused}
              via={via.current}
              lead={<Avatar name={name} />}
              primary={name}
              secondary={`${entry.sender_email} · ${plural(entry.message_count, "message")} · ${entry.latest_subject}`}
              time={<span>{formatWhen(entry.latest_at)}</span>}
              verbs={DECISIONS.map(({ disposition, label, key, Icon }) => (
                <RowVerb
                  key={disposition}
                  keys={key}
                  label={label}
                  Icon={Icon}
                  ariaLabel={`${label} ${entry.sender_email}`}
                  onClick={() => {
                    if (!decide.isPending) decide.mutate({ entry, disposition });
                  }}
                  className={cn(decide.isPending && "pointer-events-none opacity-50")}
                />
              ))}
              renderMain={(content, className) => (
                <button
                  type="button"
                  className={className}
                  aria-label={`Focus ${entry.sender_email}`}
                  onClick={() => {
                    via.current = "pointer";
                    setFocused(index);
                  }}
                >
                  {content}
                </button>
              )}
            />
          );
        })}
      </ul>
    </>
  );
}

function ScreenerDecisions({ accountId }: { accountId: string }) {
  const qc = useQueryClient();
  const decisions = useQuery({
    queryKey: ["screener-decisions", accountId],
    queryFn: () => fetchScreenerDecisions(accountId),
  });
  const decisionsHeld = useHeldPending(decisions.isPending);
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

  if (decisions.isPending || decisionsHeld) return <PageSkeleton label="Loading decisions" />;
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
