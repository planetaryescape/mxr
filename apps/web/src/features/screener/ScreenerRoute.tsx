import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Shield, Users } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import {
  clearScreenerDecision,
  fetchScreenerDecisions,
  fetchScreenerQueue,
  setScreenerDecision,
  type ScreenerDisposition,
} from "./api";
import { fetchAccounts } from "@/features/accounts/api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useShortcutScope } from "@/hooks/useShortcutScope";

export function ScreenerRoute() {
  const [tab, setTab] = useState<"queue" | "decisions">("queue");
  const [accountId, setAccountId] = useState<string | null>(null);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountList = accounts.data?.accounts ?? [];
  // Default to the first account until the user picks one explicitly.
  const activeAccountId = accountId ?? accountList[0]?.account_id ?? null;
  const account = accountList.find((item) => item.account_id === activeAccountId);

  // Keyboard triage only makes sense while the queue tab is active.
  useShortcutScope("screener", tab === "queue");

  if (!account)
    return (
      <EmptyState
        icon={Users}
        title="No account for screener"
        description="Connect an account first."
      />
    );

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex flex-wrap items-center gap-3 border-b border-border px-6 py-4">
        <div className="flex-1">
          <h1 className="text-xl font-semibold tracking-tight">Screener</h1>
          <p className="text-2xs text-muted-foreground">
            Triage unknown senders before they become inbox rules.
          </p>
        </div>
        {accountList.length > 1 ? (
          <Select value={activeAccountId ?? undefined} onValueChange={setAccountId}>
            <SelectTrigger className="h-8 w-[220px] bg-card text-xs" aria-label="Screener account">
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
        ) : (
          <span className="text-2xs text-muted-foreground">{account.email}</span>
        )}
      </header>
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as "queue" | "decisions")}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="mx-4 mt-3 w-fit">
          <TabsTrigger value="queue">Queue</TabsTrigger>
          <TabsTrigger value="decisions">Decisions</TabsTrigger>
        </TabsList>
        <TabsContent value="queue" className="min-h-0 flex-1 overflow-auto">
          <ScreenerQueue accountId={account.account_id} active={tab === "queue"} />
        </TabsContent>
        <TabsContent value="decisions" className="min-h-0 flex-1 overflow-auto">
          <ScreenerDecisions accountId={account.account_id} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function ScreenerQueue({ accountId, active }: { accountId: string; active: boolean }) {
  const qc = useQueryClient();
  const [focused, setFocused] = useState(0);
  const queue = useQuery({
    queryKey: ["screener", accountId],
    queryFn: () => fetchScreenerQueue(accountId),
    enabled: Boolean(accountId),
  });
  const decide = useMutation({
    mutationFn: ({
      senderEmail,
      disposition,
    }: {
      senderEmail: string;
      disposition: ScreenerDisposition;
    }) => setScreenerDecision({ accountId, senderEmail, disposition }),
    onSuccess: () => {
      toast.success("Screener decision saved");
      void qc.invalidateQueries({ queryKey: ["screener", accountId] });
      void qc.invalidateQueries({ queryKey: ["screener-decisions", accountId] });
    },
  });
  const rows = useMemo(() => queue.data?.entries ?? [], [queue.data?.entries]);

  useEffect(() => {
    if (focused >= rows.length) setFocused(Math.max(0, rows.length - 1));
  }, [focused, rows.length]);

  useEffect(() => {
    if (!active) return;
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      if (target instanceof HTMLElement) {
        if (target.closest("input, textarea, select, [contenteditable=true]")) return;
      }
      if (rows.length === 0 || decide.isPending) return;
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        setFocused((current) => Math.min(rows.length - 1, current + 1));
        return;
      }
      if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        setFocused((current) => Math.max(0, current - 1));
        return;
      }
      const disposition = dispositionForKey(event.key);
      if (!disposition) return;
      const row = rows[focused];
      if (!row) return;
      event.preventDefault();
      decide.mutate({ senderEmail: row.sender_email, disposition });
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active, decide, focused, rows]);

  if (rows.length === 0)
    return (
      <EmptyState
        icon={Shield}
        title="Queue empty"
        description="No unknown senders waiting for triage."
      />
    );

  return (
    <div className="p-4">
      <div className="overflow-hidden rounded-xl border border-border bg-surface">
        {rows.map((entry, index) => (
          <div
            key={entry.sender_email}
            className={
              index === focused
                ? "grid gap-3 border-b border-border bg-accent/70 px-4 py-3 text-accent-foreground ring-1 ring-ring/70 last:border-b-0 md:grid-cols-[1fr_auto]"
                : "grid gap-3 border-b border-border px-4 py-3 last:border-b-0 md:grid-cols-[1fr_auto]"
            }
            aria-current={index === focused ? "true" : undefined}
          >
            <div>
              <div className="text-sm font-medium">{entry.display_name ?? entry.sender_email}</div>
              <div className="text-2xs text-muted-foreground">
                {entry.sender_email} · {entry.message_count} messages · {entry.latest_subject}
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              {(["allow", "deny", "feed", "paper_trail"] as const).map((disposition) => (
                <Button
                  key={disposition}
                  variant={disposition === "deny" ? "destructive" : "outline"}
                  size="sm"
                  onClick={() => decide.mutate({ senderEmail: entry.sender_email, disposition })}
                >
                  {disposition.replace("_", " ")}
                </Button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ScreenerDecisions({ accountId }: { accountId: string }) {
  const qc = useQueryClient();
  const decisions = useQuery({
    queryKey: ["screener-decisions", accountId],
    queryFn: () => fetchScreenerDecisions(accountId),
    enabled: Boolean(accountId),
  });
  const clear = useMutation({
    mutationFn: (senderEmail: string) => clearScreenerDecision({ accountId, senderEmail }),
    onSuccess: () => {
      toast.success("Decision cleared");
      void qc.invalidateQueries({ queryKey: ["screener-decisions", accountId] });
      void qc.invalidateQueries({ queryKey: ["screener", accountId] });
    },
    onError: (error) => toast.error("Clear failed", { description: error.message }),
  });
  const rows = decisions.data?.decisions ?? [];

  if (decisions.isLoading)
    return <div className="p-6 text-xs text-muted-foreground">Loading decisions...</div>;
  if (rows.length === 0)
    return (
      <EmptyState
        icon={Shield}
        title="No decisions yet"
        description="Senders you allow, deny, feed, or paper-trail show up here."
      />
    );

  return (
    <div className="p-4">
      <div className="overflow-hidden rounded-xl border border-border bg-surface">
        {rows.map((decision) => (
          <div
            key={decision.sender_email}
            className="grid gap-3 border-b border-border px-4 py-3 last:border-b-0 md:grid-cols-[1fr_auto]"
          >
            <div>
              <div className="text-sm font-medium">{decision.sender_email}</div>
              <div className="text-2xs text-muted-foreground">
                {decision.disposition.replace("_", " ")}
                {decision.route_label ? ` → ${decision.route_label}` : ""} ·{" "}
                {new Date(decision.decided_at).toLocaleString()}
              </div>
            </div>
            <Button
              variant="ghost"
              size="sm"
              disabled={clear.isPending}
              onClick={() => clear.mutate(decision.sender_email)}
            >
              Clear
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}

function dispositionForKey(key: string): ScreenerDisposition | null {
  switch (key.toLowerCase()) {
    case "a":
      return "allow";
    case "d":
      return "deny";
    case "f":
      return "feed";
    case "p":
      return "paper_trail";
    default:
      return null;
  }
}
