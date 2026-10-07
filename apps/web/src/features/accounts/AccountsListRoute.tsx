import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { Plus, UserCog } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { disableAccount, fetchAccounts, setDefaultAccount } from "./api";
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList, RuledRow } from "@/components/PageParts";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import type { RuntimeAccount } from "@/features/compose/api";
import { SyncHealthWords } from "@/features/freshness/AccountSyncHealth";

export function AccountsListRoute() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [confirmDisable, setConfirmDisable] = useState<RuntimeAccount | null>(null);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const makeDefault = useMutation({
    mutationFn: setDefaultAccount,
    onSuccess: () => {
      toast.success("Default account updated");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) => toast.error("Could not change the default", { description: error.message }),
  });
  const disable = useMutation({
    mutationFn: disableAccount,
    onSuccess: () => {
      toast.success("Account disabled");
      setConfirmDisable(null);
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) =>
      toast.error("Could not disable the account", { description: error.message }),
  });
  const rows = accounts.data?.accounts ?? [];

  return (
    <Page
      title="Accounts"
      description="Mailboxes the daemon syncs, and which one sends by default."
      actions={
        <Button size="sm" asChild>
          <Link to="/accounts/$key" params={{ key: "new" }}>
            <Plus className="size-3" />
            Add account
          </Link>
        </Button>
      }
    >
      {accounts.isPending ? (
        <PageSkeleton rows={3} label="Loading accounts" />
      ) : accounts.isError ? (
        <PageError
          title="Accounts unavailable"
          error={accounts.error}
          onRetry={() => void accounts.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          icon={<UserCog className="size-5" />}
          title="No accounts yet"
          body="Connect Gmail, Outlook or any IMAP mailbox to start syncing."
          action={
            <Button size="sm" asChild>
              <Link to="/onboarding">Add an account</Link>
            </Button>
          }
        />
      ) : (
        <RuledList label="Accounts">
          {rows.map((account) => {
            const key = account.key ?? account.account_id;
            return (
              <RuledRow
                key={account.account_id}
                title={
                  <span className="flex items-center gap-2">
                    {account.enabled ? null : (
                      <span
                        aria-hidden="true"
                        className="size-1.5 rounded-full bg-muted-foreground"
                      />
                    )}
                    {account.name || account.email}
                    {account.enabled ? <SyncHealthWords accountId={account.account_id} /> : null}
                    {account.is_default ? (
                      <span className="rounded bg-primary-muted px-1.5 font-mono text-2xs text-primary">
                        default
                      </span>
                    ) : null}
                    {!account.enabled ? (
                      <span className="font-mono text-2xs text-muted-foreground">disabled</span>
                    ) : null}
                  </span>
                }
                meta={`${account.email} · ${account.provider_kind} · ${account.capabilities?.supports_send ? "can send" : "read only"}`}
                onOpen={() => void navigate({ to: "/accounts/$key", params: { key } })}
                openLabel={`Manage ${account.email}`}
                actions={
                  <>
                    {!account.is_default ? (
                      <Button
                        variant="outline"
                        size="xs"
                        disabled={!account.key || makeDefault.isPending}
                        onClick={() => account.key && makeDefault.mutate(account.key)}
                      >
                        Make default
                      </Button>
                    ) : null}
                    {account.enabled ? (
                      <Button
                        variant="ghost"
                        size="xs"
                        disabled={!account.key || disable.isPending}
                        onClick={() => setConfirmDisable(account)}
                      >
                        Disable
                      </Button>
                    ) : null}
                  </>
                }
              />
            );
          })}
        </RuledList>
      )}
      <AlertDialog
        open={confirmDisable !== null}
        onOpenChange={(open) => !open && setConfirmDisable(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Disable {confirmDisable?.email}?</AlertDialogTitle>
            <AlertDialogDescription>
              The daemon stops syncing and sending for this account. Local mail stays on disk.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={disable.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={disable.isPending}
              onClick={(event) => {
                event.preventDefault();
                if (confirmDisable?.key) disable.mutate(confirmDisable.key);
              }}
            >
              Disable
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}
