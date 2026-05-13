import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useParams } from "@tanstack/react-router";

import { fetchAccounts } from "@/features/accounts/api";
import { fetchSenderProfile } from "@/features/mailbox/api";

export const Route = createFileRoute("/sender/$address")({
  component: SenderProfilePage,
});

function SenderProfilePage() {
  const { address } = useParams({ from: "/sender/$address" });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountId = accounts.data?.accounts[0]?.account_id ?? "";
  const profile = useQuery({
    queryKey: ["sender-profile", accountId, address],
    queryFn: () => fetchSenderProfile({ accountId, email: address }),
    enabled: Boolean(accountId && address),
  });

  if (!accountId)
    return (
      <div className="p-6 text-sm text-muted-foreground">
        Add an account before opening a sender profile.
      </div>
    );

  return (
    <div className="space-y-3 p-6">
      <header>
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Sender profile
        </div>
        <h1 className="break-words text-xl font-semibold tracking-tight">{address}</h1>
      </header>
      {profile.isLoading ? (
        <div className="text-xs text-muted-foreground">Loading…</div>
      ) : profile.isError ? (
        <div className="text-xs text-destructive">{profile.error.message}</div>
      ) : (
        <pre className="overflow-auto rounded-md border border-border bg-muted/40 p-4 font-mono text-2xs">
          {JSON.stringify(profile.data, null, 2)}
        </pre>
      )}
    </div>
  );
}
