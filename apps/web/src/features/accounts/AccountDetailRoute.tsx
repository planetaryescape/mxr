import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { CheckCircle2, KeyRound, RefreshCw, Trash2 } from "lucide-react";
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
import { claimAccountReauthRequest } from "./reauthRequest";
import { OnboardingRoute } from "@/features/onboarding/OnboardingRoute";
import type { RuntimeAccount } from "@/features/compose/api";
import { EmptyState } from "@/components/EmptyState";
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
import { Card } from "@/components/ui/card";
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
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["accounts"] });
    void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] });
    toast.success("Refreshed");
  };
  const addAlias = useMutation({
    mutationFn: () => addAccountAddress(account?.account_id ?? "", alias),
    onSuccess: () => {
      setAlias("");
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] });
    },
  });
  const removeAlias = useMutation({
    mutationFn: (email: string) => removeAccountAddress(account?.account_id ?? "", email),
    onSuccess: () =>
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] }),
  });
  const setPrimaryAlias = useMutation({
    mutationFn: (email: string) => setPrimaryAccountAddress(account?.account_id ?? "", email),
    onSuccess: () => {
      toast.success("Primary address updated");
      void qc.invalidateQueries({ queryKey: ["account-addresses", account?.account_id] });
    },
  });
  const makeDefault = useMutation({
    mutationFn: () => setDefaultAccount(account?.key ?? keyParam),
    onSuccess: () => {
      toast.success("Default account updated");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
  });
  const disable = useMutation({
    mutationFn: () => disableAccount(account?.key ?? keyParam),
    onSuccess: () => {
      toast.success("Account disabled");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
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
      await navigate({ to: "/accounts" });
    },
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

  if (accounts.isLoading)
    return <div className="p-6 text-xs text-muted-foreground">Loading account...</div>;
  if (accounts.isError)
    return (
      <EmptyState
        icon={RefreshCw}
        title="Account unavailable"
        description={accounts.error.message}
      />
    );
  if (!account) return <EmptyState title="Account not found" description={keyParam} />;

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-6 py-4">
        <div className="flex-1">
          <h1 className="text-xl font-semibold tracking-tight">{account.name || account.email}</h1>
          <p className="text-2xs text-muted-foreground">
            {account.email} · {account.provider_kind}
          </p>
        </div>
        <Button variant="outline" onClick={() => test.mutate()} disabled={test.isPending}>
          Test connection
        </Button>
        <Button variant="outline" onClick={refresh}>
          <RefreshCw className="size-3" />
          Refresh
        </Button>
        <Button variant="outline" onClick={() => repair.mutate()} disabled={repair.isPending}>
          Repair
        </Button>
        <Button
          variant="outline"
          onClick={() => makeDefault.mutate()}
          disabled={account.is_default || makeDefault.isPending}
        >
          <CheckCircle2 className="size-3" />
          Default
        </Button>
        <Button
          variant="outline"
          onClick={() => reauth.mutate()}
          disabled={!isOauthAccount(account) || reauth.isPending}
        >
          <KeyRound className="size-3" />
          Re-auth
        </Button>
        <Button variant="ghost" onClick={() => disable.mutate()} disabled={disable.isPending}>
          Disable
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="destructive" disabled={remove.isPending}>
              <Trash2 className="size-3" />
              Remove
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Remove account config?</AlertDialogTitle>
              <AlertDialogDescription>
                This removes {account.name || account.email} from mxr. Local mail data is only
                purged when the remove option below is enabled.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                disabled={remove.isPending}
                onClick={() => remove.mutate()}
              >
                Remove
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </header>
      <main className="grid gap-4 p-6 lg:grid-cols-2">
        {authSession.data?.session ? (
          <Card className="p-4 lg:col-span-2">
            <h2 className="mb-2 text-sm font-semibold">OAuth session</h2>
            <div
              className={`whitespace-pre-wrap text-xs ${
                authSession.data.session.error ? "text-destructive" : "text-muted-foreground"
              }`}
            >
              {authSession.data.session.error ??
                authSession.data.session.message ??
                authSession.data.session.state}
            </div>
            {authSession.data.session.user_code ? (
              <div className="mt-2 font-mono text-2xl tracking-widest text-primary">
                {authSession.data.session.user_code}
              </div>
            ) : null}
            {(authSession.data.session.verification_uri ?? authSession.data.session.auth_url) ? (
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    const s = authSession.data?.session;
                    const url = s?.verification_uri ?? s?.auth_url;
                    if (url) window.open(url, "_blank", "noopener,noreferrer");
                  }}
                >
                  Open sign-in
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    const s = authSession.data?.session;
                    const url = s?.verification_uri ?? s?.auth_url;
                    if (url) {
                      void navigator.clipboard?.writeText(url);
                      toast.success("Sign-in link copied");
                    }
                  }}
                >
                  Copy sign-in link
                </Button>
              </div>
            ) : null}
            <Button
              className="mt-3"
              disabled={authSession.data.session.state !== "authorized" || completeReauth.isPending}
              onClick={() => completeReauth.mutate()}
            >
              Complete re-auth
            </Button>
          </Card>
        ) : null}
        <Card className="p-4">
          <h2 className="mb-3 text-sm font-semibold">Capabilities</h2>
          <pre className="overflow-auto rounded bg-muted p-3 text-2xs">
            {JSON.stringify(account.capabilities, null, 2)}
          </pre>
        </Card>
        <Card className="p-4">
          <h2 className="mb-3 text-sm font-semibold">Aliases</h2>
          <div className="mb-3 flex gap-2">
            <Input
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              placeholder="alias@example.com"
            />
            <Button
              onClick={() => addAlias.mutate()}
              disabled={!alias.trim() || addAlias.isPending}
            >
              Add
            </Button>
          </div>
          <div className="divide-y divide-border">
            {(addresses.data?.addresses ?? []).map((address) => (
              <div key={address.email} className="flex items-center justify-between py-2 text-xs">
                <span>
                  {address.email}
                  {address.primary ? " · primary" : ""}
                </span>
                <Button variant="ghost" size="sm" onClick={() => removeAlias.mutate(address.email)}>
                  Remove
                </Button>
                {!address.primary ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setPrimaryAlias.mutate(address.email)}
                  >
                    Primary
                  </Button>
                ) : null}
              </div>
            ))}
          </div>
        </Card>
        <Card className="p-4 lg:col-span-2">
          <h2 className="mb-3 text-sm font-semibold">Remove options</h2>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Checkbox
              id="purge-local-data"
              checked={purgeLocalData}
              onCheckedChange={(checked) => setPurgeLocalData(checked === true)}
            />
            <Label htmlFor="purge-local-data" className="text-xs text-muted-foreground">
              Purge local data for this account when removing config.
            </Label>
          </div>
        </Card>
      </main>
    </div>
  );
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
