import { createFileRoute } from "@tanstack/react-router";

import { DeliveriesRoute } from "@/features/deliveries/DeliveriesRoute";

export const Route = createFileRoute("/deliveries")({
  component: DeliveriesRoute,
});
