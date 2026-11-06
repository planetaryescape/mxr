import { useMutation, useQuery } from "@tanstack/react-query";
import { ShieldAlert } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { fetchSubscriptions, unsubscribeSubscription, type SubscriptionSummary } from "./api";
import { Segmented, useDrillToSearch } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import { PageEmpty, PageError, PageNote, PageSkeleton } from "@/components/PageParts";
import { Row, RowVerb } from "@/components/Row";
import { useLitRow } from "@/components/useLitRow";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { formatWhen, plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useHeldPending } from "@/hooks/useDelayedPending";

type Sort = "low-open" | "volume" | "recent";

const SORTS: { id: Sort; label: string }[] = [
  { id: "low-open", label: "Least opened" },
  { id: "volume", label: "Volume" },
  { id: "recent", label: "Recent" },
];

export function SubscriptionsDashboard() {
  const account = useUiPrefs((s) => s.accountScope) ?? undefined;
  const drill = useDrillToSearch();
  const [sort, setSort] = useState<Sort>("low-open");
  const [confirm, setConfirm] = useState<SubscriptionSummary | null>(null);
  const subscriptions = useQuery({
    queryKey: ["subscriptions", account ?? "all"],
    queryFn: () => fetchSubscriptions(100, account),
  });
  const subscriptionsHeld = useHeldPending(subscriptions.isPending);
  const unsubscribe = useMutation({
    mutationFn: (messageId: string) => unsubscribeSubscription(messageId),
    onSuccess: () => {
      toast.success("Unsubscribe requested");
      setConfirm(null);
    },
    onError: (error) => toast.error("Unsubscribe not finished", { description: error.message }),
  });
  const rows = sortSubscriptions(subscriptions.data?.subscriptions ?? [], sort);
  const { rowProps } = useLitRow(rows.map(rowId));

  return (
    <>
      <PageSection
        title="Newsletters"
        description={subscriptions.data ? plural(rows.length, "sender") : undefined}
        actions={<Segmented value={sort} options={SORTS} onChange={setSort} label="Sort senders" />}
      >
        <PageNote>
          Bulk senders ranked by how rarely you open them. Unsubscribe uses the newest message's
          list-unsubscribe header.
        </PageNote>
        {subscriptions.isPending || subscriptionsHeld ? (
          <PageSkeleton label="Loading subscriptions" />
        ) : subscriptions.isError ? (
          <PageError
            title="Subscriptions unavailable"
            error={subscriptions.error}
            onRetry={() => void subscriptions.refetch()}
          />
        ) : rows.length === 0 ? (
          <PageEmpty
            title="No subscriptions found"
            body="Mailing-list senders appear here once they have been synced."
          />
        ) : (
          <ul aria-label="Subscriptions">
            {rows.map((row) => {
              const id = rowId(row);
              return (
                <Row
                  accountId={row.account_id}
                  key={id}
                  {...rowProps(id)}
                  primary={row.sender_name || row.sender_email}
                  secondary={`${plural(row.message_count, "message")} · ${openRateLabel(row)} opened · ${row.latest_subject ?? ""}`}
                  time={<span>{formatWhen(row.latest_date)}</span>}
                  verbs={
                    row.latest_message_id ? (
                      <RowVerb
                        label="Unsubscribe"
                        ariaLabel={`Unsubscribe from ${row.sender_name || row.sender_email}`}
                        onClick={() => setConfirm(row)}
                      />
                    ) : undefined
                  }
                  renderMain={(content, className) => (
                    <button
                      type="button"
                      className={className}
                      aria-label={`Search mail from ${row.sender_email}`}
                      onClick={() => drill(`from:${row.sender_email}`)}
                    >
                      {content}
                    </button>
                  )}
                />
              );
            })}
          </ul>
        )}
      </PageSection>
      <AlertDialog open={Boolean(confirm)} onOpenChange={(open) => !open && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Unsubscribe from {confirm?.sender_name || confirm?.sender_email}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              mxr uses the newest message's unsubscribe method. That may send a request through the
              daemon or open the sender's page.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={unsubscribe.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={!confirm?.latest_message_id || unsubscribe.isPending}
              onClick={(event) => {
                event.preventDefault();
                if (confirm?.latest_message_id) unsubscribe.mutate(confirm.latest_message_id);
              }}
            >
              <ShieldAlert className="size-3" />
              Unsubscribe
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

const rowId = (row: SubscriptionSummary) => `${row.account_id ?? ""}:${row.sender_email}`;

function openRate(row: SubscriptionSummary): number {
  if (!row.message_count) return 0;
  return (row.opened_count ?? 0) / row.message_count;
}

function openRateLabel(row: SubscriptionSummary): string {
  return `${Math.round(openRate(row) * 100)}%`;
}

function sortSubscriptions(rows: SubscriptionSummary[], sort: Sort) {
  return rows.toSorted((a, b) => {
    if (sort === "volume") return b.message_count - a.message_count;
    if (sort === "recent")
      return String(b.latest_date ?? "").localeCompare(String(a.latest_date ?? ""));
    return openRate(a) - openRate(b) || b.message_count - a.message_count;
  });
}
