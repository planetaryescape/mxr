import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { CheckCircle2, KeyRound, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import {
  addAccountAddress,
  completeAuthSession,
  fetchAccountAddresses,
  fetchAccounts,
  fetchAuthSession,
  removeAccount,
  removeAccountAddress,
  repairAccount,
  setDefaultAccount,
  setPrimaryAccountAddress,
  startAuthSession,
  disableAccount,
  testAccount,
  type AccountConfig,
} from "./api";
import { describeAuthSession, isTerminalAuthState } from "./authSession";
import { claimAccountReauthRequest } from "./reauthRequest";
import { OnboardingRoute } from "@/features/onboarding/OnboardingRoute";
import type { RuntimeAccount } from "@/features/compose/api";
import { Page, PageSection } from "@/components/Page";
import {
  FactList,
  PageEmpty,
  PageError,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function AccountDetailRoute() {
  const { key } = useParams({ from: "/accounts/$key" });
  if (key === "new") return <OnboardingRoute />;
  return <AccountDetail keyParam={key} />;
}

function AccountDetail({ keyParam }: { keyParam: string }) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const account = accounts.data?.accounts.find(
    (item) => item.key === keyParam || item.account_id === keyParam,
  );
  const addresses = useQuery({
    queryKey: ["account-addresses", account?.account_id],
    queryFn: () => fetchAccountAddresses(account?.account_id ?? ""),
    enabled: Boolean(account?.account_id),
  });
  const [alias, setAlias] = useState("");
  const [purgeLocalData, setPurgeLocalData] = useState(false);
  const [authSessionId, setAuthSessionId] = useState<string | null>(null);
  const autoReauthStarted = useRef(false);
  const authSession = useQuery({
    queryKey: ["auth-session", authSessionId],
    queryFn: () => fetchAuthSession(authSessionId ?? ""),
    enabled: Boolean(authSessionId),
    refetchInterval: (query) => (query.state.data?.session.state === "authorized" ? false : 1500),
  });
  const test = useMutation({
    mutationFn: () => testAccount(accountConfig(account)),
    onSuccess: (result) => toast.success(result.result.summary || "Connection OK"),
    onError: (error) => toast.error("Test failed", { description: error.message }),
  });
  const repair = useMutation({
    mutationFn: () => repairAccount(accountConfig(account)),
    onSuccess: () => {
      toast.success("Account repair triggered");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) => toast.error("Repair failed", { description: error.message }),
  });
  const addAlias = useMutation({
    mutationFn: () => addAccountAddress(account?.account_id ?? "", alias),
    onSuccess: () => {
      setAlias("");
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] });
    },
    onError: (error) => toast.error("Could not add the address", { description: error.message }),
  });
  const removeAlias = useMutation({
    mutationFn: (email: string) => removeAccountAddress(account?.account_id ?? "", email),
    onSuccess: () =>
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] }),
    onError: (error) => toast.error("Could not remove the address", { description: error.message }),
  });
  const setPrimaryAlias = useMutation({
    mutationFn: (email: string) => setPrimaryAccountAddress(account?.account_id ?? "", email),
    onSuccess: () => {
      toast.success("Primary address updated");
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] });
    },
    onError: (error) =>
      toast.error("Could not change the primary address", { description: error.message }),
  });
  const makeDefault = useMutation({
    mutationFn: () => setDefaultAccount(account?.key ?? keyParam),
    onSuccess: () => {
      toast.success("Default account updated");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) => toast.error("Could not change the default", { description: error.message }),
  });
  const disable = useMutation({
    mutationFn: () => disableAccount(account?.key ?? keyParam),
    onSuccess: () => {
      toast.success("Account disabled");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) =>
      toast.error("Could not disable the account", { description: error.message }),
  });
  const reauth = useMutation({
    mutationFn: () => startAuthSession(accountConfig(account), true),
    onSuccess: (result) => {
      setAuthSessionId(result.session.session_id);
      if (result.session.verification_uri || result.session.auth_url) {
        window.open(
          result.session.verification_uri ?? result.session.auth_url,
          "_blank",
          "noopener,noreferrer",
        );
      }
      toast.success("Auth session started");
    },
    onError: (error) => toast.error("Reauth failed", { description: error.message }),
  });
  const completeReauth = useMutation({
    mutationFn: () => completeAuthSession(authSessionId ?? ""),
    onSuccess: () => {
      toast.success("Reauthorization saved");
      setAuthSessionId(null);
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) => toast.error("Reauthorization failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => removeAccount(keyParam, purgeLocalData),
    onSuccess: async () => {
      toast.success("Account removed");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      await navigate({ to: "/accounts" });
    },
    onError: (error) => toast.error("Could not remove the account", { description: error.message }),
  });

  useEffect(() => {
    autoReauthStarted.current = false;
  }, [keyParam]);

  useEffect(() => {
    if (!account || authSessionId || autoReauthStarted.current || !isOauthAccount(account)) return;
    if (!claimAccountReauthRequest(account)) return;
    autoReauthStarted.current = true;
    reauth.mutate();
  }, [account, authSessionId, reauth]);

  if (accounts.isPending)
    return (
      <Page title="Account" eyebrow="Accounts">
        <PageSkeleton rows={4} label="Loading account" />
      </Page>
    );
  if (accounts.isError)
    return (
      <Page title="Account" eyebrow="Accounts">
        <PageError
          title="Account unavailable"
          error={accounts.error}
          onRetry={() => void accounts.refetch()}
        />
      </Page>
    );
  if (!account)
    return (
      <Page title="Account not found" eyebrow="Accounts">
        <PageEmpty
          title={`No account called "${keyParam}"`}
          body="It may have been removed from another client."
          action={
            <Button size="sm" variant="outline" onClick={() => void navigate({ to: "/accounts" })}>
              All accounts
            </Button>
          }
        />
      </Page>
    );

  const session = authSession.data?.session;
  const authStatus = describeAuthSession(session, providerName(account));
  const signInUrl = session?.verification_uri ?? session?.auth_url;
  const capabilities = Object.entries(account.capabilities ?? {});

  return (
    <Page
      eyebrow="Accounts"
      title={account.name || account.email}
      description={`${account.email} · ${account.provider_kind}${account.is_default ? " · default" : ""}${account.enabled ? "" : " · disabled"}`}
      actions={
        <>
          <Button
            variant="outline"
            size="sm"
            onClick={() => test.mutate()}
            disabled={test.isPending}
          >
            {test.isPending ? "Testing…" : "Test connection"}
          </Button>
          {isOauthAccount(account) ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => reauth.mutate()}
              disabled={reauth.isPending || Boolean(authSessionId)}
            >
              <KeyRound className="size-3" />
              Sign in again
            </Button>
          ) : null}
          {!account.is_default ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => makeDefault.mutate()}
              disabled={makeDefault.isPending}
            >
              <CheckCircle2 className="size-3" />
              Make default
            </Button>
          ) : null}
        </>
      }
    >
      {session ? (
        <PageSection title="Sign in">
          {session.user_code ? (
            <div className="mb-2 font-mono text-2xl tracking-widest text-primary">
              {session.user_code}
            </div>
          ) : null}
          <p
            role="status"
            className={
              authStatus.tone === "error"
                ? "text-[13px] text-destructive"
                : authStatus.tone === "ready"
                  ? "text-[13px] text-success"
                  : "text-[13px] text-muted-foreground"
            }
          >
            {authStatus.text}
          </p>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {signInUrl && !isTerminalAuthState(session.state) ? (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => window.open(signInUrl, "_blank", "noopener,noreferrer")}
                >
                  Open sign-in
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    navigator.clipboard
                      ?.writeText(signInUrl)
                      .then(() => toast.success("Sign-in link copied"))
                      .catch((error: Error) =>
                        toast.error("Copy failed", { description: error.message }),
                      );
                  }}
                >
                  Copy sign-in link
                </Button>
              </>
            ) : null}
            <Button
              size="sm"
              disabled={session.state !== "authorized" || completeReauth.isPending}
              onClick={() => completeReauth.mutate()}
            >
              Finish sign-in
            </Button>
          </div>
        </PageSection>
      ) : null}

      <PageSection title="Connection">
        <FactList
          facts={[
            ["Provider", account.provider_kind],
            ["Sync", account.sync_kind ?? account.provider_kind],
            ["Send", account.send_kind ?? (account.capabilities?.supports_send ? "yes" : "no")],
            ["Status", account.enabled ? "enabled" : "disabled"],
            ...capabilities.map(([name, value]): [string, string] => [
              name.replace(/_/g, " "),
              value ? "yes" : "no",
            ]),
          ]}
        />
        <div className="mt-3 flex gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => repair.mutate()}
            disabled={repair.isPending}
          >
            Repair sync state
          </Button>
        </div>
      </PageSection>

      <PageSection title="Send-as addresses" description="Extra addresses you can send from.">
        <form
          className="mb-3 flex max-w-md gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (alias.trim()) addAlias.mutate();
          }}
        >
          <Input
            aria-label="New address"
            value={alias}
            onChange={(event) => setAlias(event.target.value)}
            placeholder="alias@example.com"
            className="h-8 text-xs"
          />
          <Button type="submit" size="sm" disabled={!alias.trim() || addAlias.isPending}>
            Add
          </Button>
        </form>
        {addresses.isPending ? (
          <PageSkeleton rows={2} label="Loading addresses" />
        ) : addresses.isError ? (
          <PageError
            title="Addresses unavailable"
            error={addresses.error}
            onRetry={() => void addresses.refetch()}
          />
        ) : (addresses.data?.addresses ?? []).length === 0 ? (
          <p className="text-[13px] text-muted-foreground">Only {account.email}.</p>
        ) : (
          <RuledList label="Send-as addresses">
            {(addresses.data?.addresses ?? []).map((address) => (
              <RuledRow
                key={address.email}
                title={address.email}
                meta={address.is_primary ? "primary" : undefined}
                actions={
                  <>
                    {!address.is_primary ? (
                      <Button
                        variant="ghost"
                        size="xs"
                        disabled={setPrimaryAlias.isPending}
                        onClick={() => setPrimaryAlias.mutate(address.email)}
                      >
                        Make primary
                      </Button>
                    ) : null}
                    <Button
                      variant="ghost"
                      size="xs"
                      disabled={removeAlias.isPending}
                      onClick={() => removeAlias.mutate(address.email)}
                    >
                      Remove
                    </Button>
                  </>
                }
              />
            ))}
          </RuledList>
        )}
      </PageSection>

      <PageSection title="Disable or remove">
        <div className="flex flex-wrap items-center gap-2">
          {account.enabled ? (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button variant="outline" size="sm" disabled={disable.isPending}>
                  Disable
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Disable {account.email}?</AlertDialogTitle>
                  <AlertDialogDescription>
                    The daemon stops syncing and sending for this account. Local mail stays on disk.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction variant="destructive" onClick={() => disable.mutate()}>
                    Disable
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          ) : null}
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button variant="destructive" size="sm" disabled={remove.isPending}>
                <Trash2 className="size-3" />
                Remove account
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Remove {account.name || account.email}?</AlertDialogTitle>
                <AlertDialogDescription>
                  mxr forgets this account. Choose whether its synced mail is deleted from this
                  machine too.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <div className="flex items-center gap-2 text-[13px]">
                <Checkbox
                  id="purge-local-data"
                  checked={purgeLocalData}
                  onCheckedChange={(checked) => setPurgeLocalData(checked === true)}
                />
                <Label htmlFor="purge-local-data" className="text-[13px] font-normal">
                  Also delete this account's local mail
                </Label>
              </div>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  variant="destructive"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate()}
                >
                  {purgeLocalData ? "Remove and delete mail" : "Remove"}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </PageSection>
    </Page>
  );
}

function providerName(account: RuntimeAccount): string {
  const kind = account.sync_kind ?? account.provider_kind;
  if (kind.includes("gmail")) return "Google";
  if (kind.includes("outlook")) return "Microsoft";
  return account.provider_kind;
}

function isOauthAccount(account: RuntimeAccount): boolean {
  const syncKind = account.sync_kind ?? account.provider_kind;
  return syncKind.includes("gmail") || syncKind.includes("outlook");
}

function accountConfig(account: RuntimeAccount | undefined): AccountConfig {
  if (!account) throw new Error("Account not loaded");
  return {
    key: account.key ?? account.account_id,
    name: account.name,
    email: account.email,
    enabled: account.enabled,
    is_default: account.is_default,
    sync: account.sync,
    send: account.send,
  };
}
