import { useQuery } from "@tanstack/react-query";

import { fetchAccountAddresses } from "@/features/accounts/api";
import { fetchAccounts } from "../api";
import type { ComposeDraftState } from "./composeDraft";

export function useSenderAccounts(draft: ComposeDraftState | null) {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts, staleTime: 60_000 });

  const runtimeAccounts = accounts.data?.accounts ?? [];
  const selectedAccount = draft
    ? runtimeAccounts.find((account) => account.account_id === draft.accountId)
    : undefined;

  const addressesQuery = useQuery({
    queryKey: ["account-addresses", selectedAccount?.account_id],
    queryFn: () => fetchAccountAddresses(selectedAccount?.account_id ?? ""),
    enabled: Boolean(selectedAccount?.account_id),
    staleTime: 60_000,
  });
  const accountAddresses: string[] = (() => {
    if (!selectedAccount) return [];
    const fetched = addressesQuery.data?.addresses ?? [];
    const emails = [selectedAccount.email, ...fetched.map((address) => address.email)].filter(
      (email) => email.length > 0,
    );
    return [...new Set(emails)];
  })();

  return { runtimeAccounts, selectedAccount, accountAddresses };
}
