import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { CheckCircle2, Plus, RefreshCw, UserCog } from "lucide-react";
import { toast } from "sonner";

import { disableAccount, fetchAccounts, setDefaultAccount } from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";

export function AccountsListRoute() {
  const qc = useQueryClient();
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const makeDefault = useMutation({
    mutationFn: setDefaultAccount,
    onSuccess: () => {
      toast.success("Default account updated");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
  });
  const disable = useMutation({
    mutationFn: disableAccount,
    onSuccess: () => {
      toast.success("Account disabled");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
  });
  if (accounts.isLoading)
    return <div className="p-6 text-xs text-muted-foreground">Loading accounts...</div>;
  if (accounts.isError)
    return (
      <EmptyState
        icon={RefreshCw}
        title="Accounts unavailable"
        description={accounts.error.message}
        action={<Button onClick={() => accounts.refetch()}>Retry</Button>}
      />
    );
  const rows = accounts.data?.accounts ?? [];
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-6 py-4">
        <div className="flex-1">
          <h1 className="text-xl font-semibold tracking-tight">Accounts</h1>
          <p className="text-2xs text-muted-foreground">
            Runtime providers, send capability, and defaults.
          </p>
        </div>
        <Button asChild>
          <Link to="/accounts/$key" params={{ key: "new" }}>
            <Plus className="size-3" />
            Add account
          </Link>
        </Button>
      </header>
      {rows.length === 0 ? (
        <EmptyState
          icon={UserCog}
          title="No accounts configured"
          description="Connect Gmail, Outlook, or IMAP to start syncing mail."
          action={
            <Button asChild>
              <Link to="/onboarding">Start onboarding</Link>
            </Button>
          }
        />
      ) : (
        <div className="p-4">
          <div className="overflow-hidden rounded-xl border border-border bg-surface">
            {rows.map((account) => (
              <div
                key={account.account_id}
                className="grid gap-3 border-b border-border px-4 py-3 last:border-b-0 md:grid-cols-[1fr_auto]"
              >
                <Link
                  to="/accounts/$key"
                  params={{ key: account.key ?? account.account_id }}
                  className="min-w-0"
                >
                  <div className="flex items-center gap-2">
                    <span className="size-2 rounded-full bg-success" />
                    <span className="text-sm font-medium">{account.name || account.email}</span>
                    {account.is_default ? (
                      <span className="rounded bg-primary-muted px-1.5 py-0.5 text-2xs text-primary">
                        default
                      </span>
                    ) : null}
                  </div>
                  <div className="mt-1 text-2xs text-muted-foreground">
                    {account.email} · {account.provider_kind} · send{" "}
                    {account.capabilities?.supports_send ? "on" : "off"}
                  </div>
                </Link>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={account.is_default || !account.key || makeDefault.isPending}
                    onClick={() => account.key && makeDefault.mutate(account.key)}
                  >
                    <CheckCircle2 className="size-3" />
                    Default
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!account.key || disable.isPending}
                    onClick={() => account.key && disable.mutate(account.key)}
                  >
                    Disable
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
