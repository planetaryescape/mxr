import { createFileRoute } from "@tanstack/react-router";

import { AccountDetailRoute } from "@/features/accounts/AccountDetailRoute";

export const Route = createFileRoute("/accounts/$key")({
  component: AccountDetailRoute,
});
