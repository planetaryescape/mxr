import { createFileRoute } from "@tanstack/react-router";

import { Page } from "@/components/Page";
import { SubscriptionsDashboard } from "@/features/analytics/SubscriptionsDashboard";

export const Route = createFileRoute("/subscriptions")({
  component: SubscriptionsRoute,
});

function SubscriptionsRoute() {
  return (
    <Page title="Subscriptions" description="Newsletters, how much you read them, and unsubscribe.">
      <SubscriptionsDashboard />
    </Page>
  );
}
