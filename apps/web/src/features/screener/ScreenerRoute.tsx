import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Shield, Users } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { fetchScreenerQueue, setScreenerDecision, type ScreenerDisposition } from "./api";
import { fetchAccounts } from "@/features/accounts/api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";

export function ScreenerRoute() {
  const qc = useQueryClient();
  const [focused, setFocused] = useState(0);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const account = accounts.data?.accounts[0];
  const queue = useQuery({
    queryKey: ["screener", account?.account_id],
    queryFn: () => fetchScreenerQueue(account?.account_id ?? ""),
    enabled: Boolean(account?.account_id),
  });
  const decide = useMutation({
    mutationFn: ({
      senderEmail,
      disposition,
    }: {
      senderEmail: string;
      disposition: ScreenerDisposition;
    }) => setScreenerDecision({ accountId: account?.account_id ?? "", senderEmail, disposition }),
    onSuccess: () => {
      toast.success("Screener decision saved");
      void qc.invalidateQueries({ queryKey: ["screener", account?.account_id] });
    },
  });
  const rows = useMemo(() => queue.data?.entries ?? [], [queue.data?.entries]);

  useEffect(() => {
    if (focused >= rows.length) setFocused(Math.max(0, rows.length - 1));
  }, [focused, rows.length]);

  useEffect(() => {
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
  }, [decide, focused, rows]);

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
      <header className="border-b border-border px-6 py-4">
        <h1 className="text-xl font-semibold tracking-tight">
          Screener
          {account?.email ? (
            <span className="ml-2 text-2xs font-normal text-muted-foreground">
              · {account.email}
            </span>
          ) : null}
        </h1>
        <p className="text-2xs text-muted-foreground">
          Triage unknown senders before they become inbox rules.
        </p>
        {(accounts.data?.accounts.length ?? 0) > 1 ? (
          <p className="mt-2 text-2xs text-warning">
            Showing the first account only. Use the CLI for cross-account screener sweeps.
          </p>
        ) : null}
      </header>
      {rows.length === 0 ? (
        <EmptyState
          icon={Shield}
          title="Queue empty"
          description="No unknown senders waiting for triage."
        />
      ) : (
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
                  <div className="text-sm font-medium">
                    {entry.display_name ?? entry.sender_email}
                  </div>
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
                      onClick={() =>
                        decide.mutate({ senderEmail: entry.sender_email, disposition })
                      }
                    >
                      {disposition.replace("_", " ")}
                    </Button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
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
