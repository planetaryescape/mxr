import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Package, RefreshCw, Check, X, ExternalLink } from "lucide-react";
import { toast } from "sonner";

import {
  fetchDeliveries,
  resolveDelivery,
  dismissDelivery,
  type Delivery,
  type DeliveryFilter,
} from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";

const FILTERS: { id: DeliveryFilter; label: string }[] = [
  { id: "active", label: "Active" },
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

const etaFmt = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

function statusLabel(status: string): string {
  return STATUS_LABELS[status] ?? status;
}

function etaText(d: Delivery): string {
  const value = d.delivered_at ?? d.eta_until ?? d.eta_from;
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return (d.delivered_at ? "Delivered " : "ETA ") + etaFmt.format(date);
}

function DeliveryCard({
  delivery,
  onResolve,
  onDismiss,
  busy,
}: {
  delivery: Delivery;
  onResolve: (id: string) => void;
  onDismiss: (id: string) => void;
  busy: boolean;
}) {
  const title = delivery.merchant || delivery.carrier || "Unknown sender";
  const itemText = delivery.items.map((i) => i.name).join(", ");
  return (
    <div className="flex items-start gap-4 border-b border-border px-6 py-4">
      <div className="mt-0.5 text-muted-foreground">
        <Package className="size-5" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="rounded bg-muted px-2 py-0.5 font-mono text-2xs uppercase tracking-wide">
            {statusLabel(delivery.status)}
          </span>
          <span className="truncate font-medium">{title}</span>
          {delivery.carrier && (
            <span className="text-2xs text-muted-foreground">· {delivery.carrier}</span>
          )}
        </div>
        {itemText && (
          <div className="mt-1 truncate text-sm text-muted-foreground">{itemText}</div>
        )}
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-muted-foreground">
          <span>{etaText(delivery)}</span>
          {delivery.order_number && <span>Order {delivery.order_number}</span>}
          {delivery.tracking_number && (
            <span className="font-mono">{delivery.tracking_number}</span>
          )}
          {delivery.tracking_url && (
            <a
              href={delivery.tracking_url}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-primary hover:underline"
            >
              Track <ExternalLink className="size-3" />
            </a>
          )}
          {delivery.thread_id && (
            <Link
              to="/m/$mailbox/$threadId"
              params={{ mailbox: "inbox", threadId: delivery.thread_id }}
              className="text-primary hover:underline"
            >
              Open email
            </Link>
          )}
        </div>
      </div>
      <div className="flex shrink-0 gap-1">
        {!delivery.delivered_at && (
          <Button
            size="sm"
            variant="ghost"
            disabled={busy}
            onClick={() => onResolve(delivery.id)}
            title="Mark delivered"
          >
            <Check className="size-4" />
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          onClick={() => onDismiss(delivery.id)}
          title="Dismiss (false positive)"
        >
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}

export function DeliveriesRoute() {
  const qc = useQueryClient();
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
    onError: (e) => toast.error("Update failed", { description: e.message }),
  });
  const dismiss = useMutation({
    mutationFn: dismissDelivery,
    onSuccess: () => {
      toast.success("Dismissed");
      void invalidate();
    },
    onError: (e) => toast.error("Update failed", { description: e.message }),
  });

  const rows = deliveries.data?.deliveries ?? [];
  const busy = resolve.isPending || dismiss.isPending;

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Packages
        </div>
        <h1 className="text-xl font-semibold tracking-tight">Deliveries</h1>
        <p className="mt-1 text-2xs text-muted-foreground">
          Detected from your mail. Resolve marks one done; dismiss hides a false positive.
        </p>
        <div className="mt-3 flex gap-1">
          {FILTERS.map((f) => (
            <Button
              key={f.id}
              size="sm"
              variant={filter === f.id ? "secondary" : "ghost"}
              onClick={() => setFilter(f.id)}
            >
              {f.label}
            </Button>
          ))}
        </div>
      </header>

      {deliveries.isLoading ? (
        <div className="p-6 text-xs text-muted-foreground">Loading deliveries…</div>
      ) : deliveries.isError ? (
        <EmptyState
          icon={RefreshCw}
          title="Deliveries unavailable"
          description={deliveries.error.message}
          action={<Button onClick={() => deliveries.refetch()}>Retry</Button>}
        />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={Package}
          title="No deliveries"
          description="Package and shipping emails will show up here as they arrive."
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          {rows.map((d) => (
            <DeliveryCard
              key={d.id}
              delivery={d}
              busy={busy}
              onResolve={resolve.mutate}
              onDismiss={dismiss.mutate}
            />
          ))}
        </div>
      )}
    </div>
  );
}
