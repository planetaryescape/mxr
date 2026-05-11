import { createFileRoute } from "@tanstack/react-router";

import { AccountsListRoute } from "@/features/accounts/AccountsListRoute";

export const Route = createFileRoute("/accounts")({
  component: AccountsListRoute,
});
