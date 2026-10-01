import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye } from "lucide-react";
import { useState, type FormEvent } from "react";
import { toast } from "sonner";

import { fetchCadenceDrift, fetchCadenceWatchlist, unwatchCadence, watchCadence } from "./api";
import { formatDays, useDrillToSearch } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import {
  PageEmpty,
  PageError,
  PageNote,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatWhen, formatRelative } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * TUI "Cadence drift": contacts you watch, flagged when they have been
 * quiet longer than their usual rhythm. Scoped to the sidebar's account
 * (the bridge falls back to the default account).
 */
export function CadenceDashboard() {
  const qc = useQueryClient();
  const drill = useDrillToSearch();
  const accountId = useUiPrefs((state) => state.accountScope);
  const [email, setEmail] = useState("");
  const [expected, setExpected] = useState("");
  const drift = useQuery({
    queryKey: ["analytics", "cadence-drift", accountId],
    queryFn: () => fetchCadenceDrift(accountId),
  });
  const watchlist = useQuery({
    queryKey: ["analytics", "cadence-watch", accountId],
    queryFn: () => fetchCadenceWatchlist(accountId),
  });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["analytics", "cadence-drift"] });
    void qc.invalidateQueries({ queryKey: ["analytics", "cadence-watch"] });
  };
  const watch = useMutation({
    mutationFn: () =>
      watchCadence({
        accountId,
        email: email.trim(),
        expectedDays: expected.trim() ? Number(expected) : null,
      }),
    onSuccess: () => {
      toast.success(`Watching ${email.trim()}`);
      setEmail("");
      setExpected("");
      refresh();
    },
    onError: (error) => toast.error("Could not watch contact", { description: error.message }),
  });
  const unwatch = useMutation({
    mutationFn: (target: string) => unwatchCadence({ accountId, email: target }),
    onSuccess: (_result, target) => {
      toast.success(`Stopped watching ${target}`);
      refresh();
    },
    onError: (error) => toast.error("Could not unwatch contact", { description: error.message }),
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (email.trim()) watch.mutate();
  };

  return (
    <div className="grid gap-x-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,22rem)]">
      <PageSection title="Overdue" description="Watched contacts quiet for longer than usual.">
        {drift.isPending ? (
          <PageSkeleton label="Loading cadence drift" />
        ) : drift.isError ? (
          <PageError
            title="Cadence drift unavailable"
            error={drift.error}
            onRetry={() => void drift.refetch()}
          />
        ) : drift.data.rows.length === 0 ? (
          <PageEmpty
            icon={<Eye className="size-5" />}
            title="Nobody is overdue"
            body={
              (watchlist.data?.entries.length ?? 0) === 0
                ? "Watch a contact to be told when they go quiet longer than usual."
                : "Every watched contact is within their usual rhythm."
            }
          />
        ) : (
          <RuledList label="Overdue contacts">
            {drift.data.rows.map((row) => (
              <RuledRow
                key={row.email}
                title={row.display_name || row.email}
                meta={`${row.email} · usually every ${formatDays(row.expected_days)} · last ${row.last_contact_at ? formatWhen(row.last_contact_at) : "never"}`}
                aside={<span className="text-warning">{formatDays(row.drift_days)} late</span>}
                onOpen={() => drill(`from:${row.email}`)}
                openLabel={`Search mail from ${row.email}`}
              />
            ))}
          </RuledList>
        )}
      </PageSection>
      <PageSection title="Watchlist">
        <form onSubmit={submit} className="mb-3 flex gap-2">
          <Input
            aria-label="Contact email to watch"
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            placeholder="person@example.com"
            className="h-8 min-w-0 flex-1 text-xs"
          />
          <Input
            aria-label="Expected days between messages (optional)"
            type="number"
            min={1}
            value={expected}
            onChange={(event) => setExpected(event.target.value)}
            placeholder="days"
            className="h-8 w-16 text-xs"
          />
          <Button type="submit" size="sm" disabled={!email.trim() || watch.isPending}>
            Watch
          </Button>
        </form>
        <PageNote>Leave days empty to learn the rhythm from past mail.</PageNote>
        {watchlist.isPending ? (
          <PageSkeleton rows={3} label="Loading watchlist" />
        ) : watchlist.isError ? (
          <PageError
            title="Watchlist unavailable"
            error={watchlist.error}
            onRetry={() => void watchlist.refetch()}
          />
        ) : watchlist.data.entries.length === 0 ? (
          <p className="py-2 text-[13px] text-muted-foreground">No watched contacts yet.</p>
        ) : (
          <RuledList label="Watched contacts">
            {watchlist.data.entries.map((entry) => (
              <RuledRow
                key={entry.email}
                title={entry.email}
                meta={`${entry.expected_days ? `every ${formatDays(entry.expected_days)}` : "learned rhythm"} · added ${formatRelative(entry.added_at)}${entry.note ? ` · ${entry.note}` : ""}`}
                actions={
                  <Button
                    variant="ghost"
                    size="xs"
                    disabled={unwatch.isPending}
                    onClick={() => unwatch.mutate(entry.email)}
                    aria-label={`Stop watching ${entry.email}`}
                  >
                    Unwatch
                  </Button>
                }
              />
            ))}
          </RuledList>
        )}
      </PageSection>
    </div>
  );
}
