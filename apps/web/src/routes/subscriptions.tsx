import { createFileRoute } from "@tanstack/react-router";

import { SubscriptionsDashboard } from "@/features/analytics/AnalyticsDashboardRoute";

export const Route = createFileRoute("/subscriptions")({
  component: SubscriptionsRoute,
});

function SubscriptionsRoute() {
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Subscriptions
        </div>
        <h1 className="text-xl font-semibold tracking-tight">Newsletter ROI</h1>
        <p className="mt-1 text-2xs text-muted-foreground">
          Bulk senders, open-rate signals, and unsubscribe actions.
        </p>
      </header>
      <main className="min-h-0 flex-1 overflow-auto p-6">
        <SubscriptionsDashboard />
      </main>
    </div>
  );
}
