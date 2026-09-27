import { Outlet, createFileRoute, useMatch } from "@tanstack/react-router";

import { AccountsListRoute } from "@/features/accounts/AccountsListRoute";

export const Route = createFileRoute("/accounts")({
  component: AccountsLayout,
});

/** `/accounts/$key` nests under this route; the list only shows without a child. */
function AccountsLayout() {
  const detail = useMatch({ from: "/accounts/$key", shouldThrow: false });
  return detail ? <Outlet /> : <AccountsListRoute />;
}
