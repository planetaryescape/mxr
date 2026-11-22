import { createFileRoute } from "@tanstack/react-router";

import { ScreenerRoute } from "@/features/screener/ScreenerRoute";
import { optionalString } from "@/lib/searchParams";

export const Route = createFileRoute("/screener")({
  validateSearch: (search: Record<string, unknown>): { account?: string } => ({
    account: optionalString(search.account),
  }),
  component: Screener,
});

function Screener() {
  const { account } = Route.useSearch();
  return <ScreenerRoute account={account} />;
}
