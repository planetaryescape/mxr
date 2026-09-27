import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Check, ExternalLink, Package, ScanSearch, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import {
  dismissDelivery,
  fetchDeliveries,
  resolveDelivery,
  scanDeliveries,
  type Delivery,
  type DeliveryFilter,
} from "./api";
import { Page, PageTabs } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

const FILTERS: { id: DeliveryFilter; label: string }[] = [
  { id: "active", label: "On the way" },
  { id: "delivered", label: "Delivered" },
  { id: "all", label: "All" },
];

const STATUS_LABELS: Record<string, string> = {
  ordered: "Ordered",
  info_received: "Label created",
  in_transit: "In transit",
  out_for_delivery: "Out for delivery",
  attempt_fail: "Delivery attempted",
  available_for_pickup: "Ready for pickup",
  delivered: "Delivered",
  exception: "Exception",
  returned: "Returned",
  expired: "Expired",
};

const ATTENTION = new Set(["exception", "attempt_fail", "returned", "expired"]);

const dayFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  month: "short",
  day: "numeric",
});

function when(delivery: Delivery): string | null {
  const value = delivery.delivered_at ?? delivery.eta_until ?? delivery.eta_from;
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return `${delivery.delivered_at ? "Delivered" : "Expected"} ${dayFormat.format(date)}`;
}

/** Carrier names come from the heuristic in lower case ("ups"). */
function carrierName(carrier: string): string {
  return carrier.length <= 4 ? carrier.toUpperCase() : carrier[0]!.toUpperCase() + carrier.slice(1);
}

export function DeliveriesRoute() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [filter, setFilter] = useState<DeliveryFilter>("active");
  const deliveries = useQuery({
    queryKey: ["deliveries", filter],
    queryFn: () => fetchDeliveries(filter),
  });
  const invalidate = () => qc.invalidateQueries({ queryKey: ["deliveries"] });

  const resolve = useMutation({
    mutationFn: resolveDelivery,
    onSuccess: () => {
      toast.success("Marked delivered");
      void invalidate();
    },
    onError: (e) => toast.error("Could not mark delivered", { description: e.message }),
  });
  const dismiss = useMutation({
    mutationFn: dismissDelivery,
    onSuccess: () => {
      toast.success("Dismissed", { description: "It stays out of this list." });
      void invalidate();
    },
    onError: (e) => toast.error("Could not dismiss", { description: e.message }),
  });
  const scan = useMutation({
    mutationFn: () => scanDeliveries(),
    onSuccess: ({ summary }) => {
      const headline =
        summary.created > 0
          ? `Found ${plural(summary.created, "new delivery", "new deliveries")}`
          : summary.updated > 0
            ? `Updated ${plural(summary.updated, "delivery", "deliveries")}`
            : "No new deliveries";
      toast.success(headline, { description: `Checked ${plural(summary.scanned, "message")}.` });
      void invalidate();
    },
    onError: (e) => toast.error("Scan failed", { description: e.message }),
  });

  const rows = deliveries.data?.deliveries ?? [];
  const busy = resolve.isPending || dismiss.isPending;
  const openEmail = (threadId: string) =>
    void navigate({ to: "/m/$mailbox/$threadId", params: { mailbox: "archive", threadId } });

  return (
    <Page
      title="Deliveries"
      description="Packages detected in your mail. Mark one delivered, or dismiss a false positive."
      actions={
        <Button variant="outline" size="sm" onClick={() => scan.mutate()} disabled={scan.isPending}>
          <ScanSearch className="size-3" />
          {scan.isPending ? "Scanning…" : "Scan for deliveries"}
        </Button>
      }
      tabs={<PageTabs label="Delivery filter" value={filter} onChange={setFilter} tabs={FILTERS} />}
    >
      {deliveries.isPending ? (
        <PageSkeleton rows={4} label="Loading deliveries" />
      ) : deliveries.isError ? (
        <PageError
          title="Deliveries unavailable"
          error={deliveries.error}
          onRetry={() => void deliveries.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          icon={<Package className="size-5" />}
          title={filter === "active" ? "Nothing on the way" : "No deliveries"}
          body="Shipping emails are picked up after each sync. Scan now to check recent mail again."
          action={
            <Button
              size="sm"
              variant="outline"
              onClick={() => scan.mutate()}
              disabled={scan.isPending}
            >
              Scan for deliveries
            </Button>
          }
        />
      ) : (
        <RuledList label="Deliveries">
          {rows.map((delivery) => {
            const title =
              delivery.merchant || (delivery.carrier ? carrierName(delivery.carrier) : "Package");
            const items = delivery.items.map((item) => item.name).join(", ");
            const eta = when(delivery);
            return (
              <li
                key={delivery.id}
                className="flex items-start gap-3 border-b border-border/60 px-2 py-3"
              >
                <Package className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-baseline gap-x-2">
                    <span className="text-[13px] font-medium">{title}</span>
                    <span
                      className={cn(
                        "font-mono text-2xs",
                        ATTENTION.has(delivery.status)
                          ? "text-warning"
                          : delivery.status === "delivered"
                            ? "text-success"
                            : "text-primary",
                      )}
                    >
                      {STATUS_LABELS[delivery.status] ?? delivery.status}
                    </span>
                    {eta ? (
                      <span className="text-[12.5px] text-muted-foreground">{eta}</span>
                    ) : null}
                  </div>
                  {items ? (
                    <div className="mt-0.5 truncate text-[12.5px] text-muted-foreground">
                      {items}
                    </div>
                  ) : null}
                  <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-2xs text-muted-foreground">
                    {delivery.carrier && delivery.merchant ? (
                      <span>{carrierName(delivery.carrier)}</span>
                    ) : null}
                    {delivery.order_number ? <span>order {delivery.order_number}</span> : null}
                    {delivery.tracking_number ? <span>{delivery.tracking_number}</span> : null}
                    {delivery.tracking_url ? (
                      <a
                        href={delivery.tracking_url}
                        target="_blank"
                        rel="noreferrer noopener"
                        className="inline-flex items-center gap-1 text-primary hover:underline"
                      >
                        Track <ExternalLink className="size-3" />
                      </a>
                    ) : null}
                    {delivery.thread_id ? (
                      <button
                        type="button"
                        className="text-primary hover:underline"
                        onClick={() => delivery.thread_id && openEmail(delivery.thread_id)}
                      >
                        Open email
                      </button>
                    ) : null}
                  </div>
                </div>
                <div className="flex shrink-0 gap-1">
                  {!delivery.delivered_at ? (
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => resolve.mutate(delivery.id)}
                      aria-label={`Mark ${title} delivered`}
                      title="Mark delivered"
                    >
                      <Check className="size-3.5" />
                    </Button>
                  ) : null}
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    disabled={busy}
                    onClick={() => dismiss.mutate(delivery.id)}
                    aria-label={`Dismiss ${title}`}
                    title="Dismiss (not a delivery)"
                  >
                    <X className="size-3.5" />
                  </Button>
                </div>
              </li>
            );
          })}
        </RuledList>
      )}
    </Page>
  );
}
