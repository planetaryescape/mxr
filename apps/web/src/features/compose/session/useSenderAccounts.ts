/*
 * Sending identity for a compose session: the runtime accounts, the one the
 * draft sends from, and the addresses (primary + aliases) it may send as.
 */

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

  // Aliases the selected account may send as (send-as). Shares the cache key
  // used by the account-detail address editor so both stay consistent.
  const addressesQuery = useQuery({
    queryKey: ["account-addresses", selectedAccount?.account_id],
    queryFn: () => fetchAccountAddresses(selectedAccount?.account_id ?? ""),
    enabled: Boolean(selectedAccount?.account_id),
    staleTime: 60_000,
  });
  // Union of the account's primary email and its configured aliases, primary
  // first (it always leads because we prepend it), deduped, so the current
  // `from` always has a matching option in the picker.
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
