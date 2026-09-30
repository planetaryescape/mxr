import { useQuery } from "@tanstack/react-query";

import { fetchAccounts } from "@/features/accounts/api";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * Names the account a conversation belongs to when it isn't the one the
 * app is scoped to, e.g. a draft's cited source opened by link, so its
 * messages are never read as the scoped account's.
 */
export function OtherAccountLine({ accountId }: { accountId: string }) {
  const scope = useUiPrefs((s) => s.accountScope);
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    staleTime: 60_000,
    enabled: scope !== null && scope !== accountId,
  });
  if (scope === null || scope === accountId) return null;
  const account = accounts.data?.accounts.find((row) => row.account_id === accountId);
  const name = account ? account.email || account.name : "another account";
  return (
    <p
      data-testid="other-account-line"
      className="border-b border-border px-5 py-1.5 text-2xs text-muted-foreground"
    >
      In {name}, not the account you're viewing.
    </p>
  );
}
