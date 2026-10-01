import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Check, CheckCircle2, Mail, Server } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import {
  cancelAuthSession,
  completeAuthSession,
  fetchAuthSession,
  gmailAccountConfig,
  imapAccountConfig,
  outlookAccountConfig,
  startAuthSession,
  testAccount,
  upsertAccount,
  type AccountConfig,
} from "@/features/accounts/api";
import { describeAuthSession, isTerminalAuthState } from "@/features/accounts/authSession";
import { Page } from "@/components/Page";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useConnectionStore } from "@/state/connectionStore";

type Provider = "gmail" | "outlook" | "imap";

export function OnboardingRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const sync = useConnectionStore((state) => state.syncProgress);
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [provider, setProvider] = useState<Provider>("gmail");
  const [email, setEmail] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [imap, setImap] = useState({
    name: "",
    email: "",
    imapHost: "",
    imapPort: 993,
    imapMaxConnections: 4,
    smtpHost: "",
    smtpPort: 587,
    username: "",
    password: "",
  });

  const authSession = useQuery({
    queryKey: ["auth-session", sessionId],
    queryFn: () => fetchAuthSession(sessionId ?? ""),
    enabled: Boolean(sessionId),
    // Stop polling once the daemon reaches a terminal state — authorized,
    // failed, or cancelled — mirroring the TUI's spawn_outlook_auth_session
    // loop. Use the daemon-supplied poll interval when present.
    refetchInterval: (query) => {
      const state = query.state.data?.session.state;
      if (state && isTerminalAuthState(state)) return false;
      const secs = query.state.data?.session.poll_interval_secs;
      return secs ? secs * 1000 : 1500;
    },
  });
  const startAuth = useMutation({
    mutationFn: (account: AccountConfig) => startAuthSession(account),
    onSuccess: (result) => {
      setSessionId(result.session.session_id);
      setStep(3);
    },
    onError: (error) => toast.error("OAuth start failed", { description: error.message }),
  });
  const cancelAuth = useMutation({
    mutationFn: () => cancelAuthSession(sessionId ?? ""),
    onSuccess: () => {
      setSessionId(null);
      setStep(2);
    },
    onError: (error) => toast.error("Cancel failed", { description: error.message }),
  });
  const completeAuth = useMutation({
    mutationFn: () => completeAuthSession(sessionId ?? ""),
    onSuccess: () => {
      toast.success("Account connected");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      setStep(4);
    },
    onError: (error) => toast.error("Auth completion failed", { description: error.message }),
  });
  const saveImap = useMutation({
    mutationFn: async () => {
      const config = imapAccountConfig(imap);
      const test = await testAccount(config);
      if (!test.result.ok) throw new Error(test.result.summary || "Account test failed");
      return upsertAccount(config);
    },
    onSuccess: () => {
      toast.success("IMAP account saved");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      setStep(4);
    },
    onError: (error) => toast.error("IMAP setup failed", { description: error.message }),
  });

  const session = authSession.data?.session;
  const providerLabel = providerTiles.find((tile) => tile.id === provider)?.label ?? provider;
  // Device flow surfaces a verification_uri; loopback surfaces auth_url.
  const signInUrl = session?.verification_uri ?? session?.auth_url;
  const authStatus = describeAuthSession(session, providerLabel);

  return (
    <Page
      eyebrow="Add an account"
      title={STEP_TITLES[step]}
      width="narrow"
      tabs={<StepIndicator step={step} />}
    >
      {step === 1 ? (
        <section>
          <p className="max-w-[60ch] text-[13px] leading-relaxed text-muted-foreground">
            mxr keeps your mail in a local SQLite store, syncs it in the background daemon, and
            searches it without a round trip. Connect a mailbox to start, or run{" "}
            <code className="font-mono text-xs text-foreground">mxr demo</code> in a terminal for a
            sample inbox with threads, attachments, newsletters, rules and analytics.
          </p>
          <Button className="mt-6" size="sm" onClick={() => setStep(2)}>
            Connect first account
          </Button>
        </section>
      ) : null}
      {step === 2 ? (
        <section>
          <div role="radiogroup" aria-label="Provider" className="grid gap-2 sm:grid-cols-3">
            {providerTiles.map((tile) => (
              <button
                key={tile.id}
                type="button"
                role="radio"
                aria-checked={provider === tile.id}
                className={cn(
                  "rounded-md border p-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  provider === tile.id
                    ? "border-primary bg-primary-muted"
                    : "border-border hover:bg-muted/40",
                )}
                onClick={() => setProvider(tile.id)}
              >
                <tile.Icon className="mb-2 size-4 text-primary" />
                <div className="text-[13px] font-medium">{tile.label}</div>
                <div className="mt-0.5 text-2xs text-muted-foreground">{tile.description}</div>
              </button>
            ))}
          </div>
          <div className="mt-5 max-w-sm space-y-1">
            <Label htmlFor="onboarding-email" className="text-xs">
              Email
            </Label>
            <Input
              id="onboarding-email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              placeholder="you@example.com"
              className="h-8 text-[13px]"
            />
          </div>
          <Button
            className="mt-4"
            size="sm"
            disabled={!email.trim() || startAuth.isPending}
            onClick={() =>
              provider === "gmail"
                ? startAuth.mutate(gmailAccountConfig(email.trim()))
                : provider === "outlook"
                  ? startAuth.mutate(outlookAccountConfig(email.trim()))
                  : (setImap({
                      ...imap,
                      email: email.trim(),
                      username: email.trim(),
                      name: email.trim(),
                    }),
                    setStep(3))
            }
          >
            Continue
          </Button>
        </section>
      ) : null}
      {step === 3 && provider !== "imap" ? (
        <section>
          <p className="text-[13px] text-muted-foreground">
            {session?.user_code
              ? "Enter this code on the verification page to let mxr read and send your mail."
              : `Sign in to ${providerLabel} in a new tab and approve access. mxr picks up the result on its own.`}
          </p>
          <div className="mt-5 border-y border-border py-5">
            {session?.user_code ? (
              <div className="mb-3 font-mono text-3xl tracking-widest text-primary">
                {session.user_code}
              </div>
            ) : null}
            <p
              role="status"
              className={cn(
                "text-[13px]",
                authStatus.tone === "error"
                  ? "text-destructive"
                  : authStatus.tone === "ready"
                    ? "text-success"
                    : "text-muted-foreground",
              )}
            >
              {authStatus.text}
            </p>
            {signInUrl && session && !isTerminalAuthState(session.state) ? (
              <div className="mt-4 flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  onClick={() => window.open(signInUrl, "_blank", "noopener,noreferrer")}
                >
                  Open {providerLabel} sign-in
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
              </div>
            ) : null}
          </div>
          <div className="mt-4 flex gap-2">
            <Button
              size="sm"
              disabled={session?.state !== "authorized" || completeAuth.isPending}
              onClick={() => completeAuth.mutate()}
            >
              <CheckCircle2 className="size-3" />
              Complete
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={cancelAuth.isPending}
              onClick={() => (sessionId ? cancelAuth.mutate() : setStep(2))}
            >
              Cancel
            </Button>
          </div>
        </section>
      ) : null}
      {step === 3 && provider === "imap" ? (
        <section>
          <div className="grid gap-3 sm:grid-cols-2">
            <Text
              label="Account name"
              value={imap.name}
              onChange={(name) => setImap({ ...imap, name })}
            />
            <Text
              label="Email"
              value={imap.email}
              onChange={(nextEmail) => setImap({ ...imap, email: nextEmail })}
            />
            <Text
              label="IMAP host"
              value={imap.imapHost}
              onChange={(imapHost) => setImap({ ...imap, imapHost })}
            />
            <NumberField
              label="IMAP port"
              value={imap.imapPort}
              onChange={(imapPort) => setImap({ ...imap, imapPort })}
            />
            <NumberField
              label="IMAP max connections"
              value={imap.imapMaxConnections}
              onChange={(imapMaxConnections) => setImap({ ...imap, imapMaxConnections })}
            />
            <Text
              label="SMTP host"
              value={imap.smtpHost}
              onChange={(smtpHost) => setImap({ ...imap, smtpHost })}
            />
            <NumberField
              label="SMTP port"
              value={imap.smtpPort}
              onChange={(smtpPort) => setImap({ ...imap, smtpPort })}
            />
            <Text
              label="Username"
              value={imap.username}
              onChange={(username) => setImap({ ...imap, username })}
            />
            <Text
              label="Password"
              type="password"
              value={imap.password}
              onChange={(password) => setImap({ ...imap, password })}
            />
          </div>
          <div className="mt-4 flex gap-2">
            <Button
              size="sm"
              disabled={
                saveImap.isPending ||
                !imap.email ||
                !imap.imapHost ||
                !imap.smtpHost ||
                imap.imapMaxConnections < 1
              }
              onClick={() => saveImap.mutate()}
            >
              {saveImap.isPending ? "Testing connection…" : "Test and save"}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setStep(2)}>
              Back
            </Button>
          </div>
        </section>
      ) : null}
      {step === 4 ? (
        <FirstSync
          onOpenInbox={() => navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } })}
          onManage={() => navigate({ to: "/accounts" })}
          sync={sync}
        />
      ) : null}
    </Page>
  );
}

const STEP_TITLES = {
  1: "Bring your mailbox local",
  2: "Choose a provider",
  3: "Sign in",
  4: "First sync",
} as const;

const STEP_LABELS = ["Welcome", "Provider", "Sign in", "First sync"];

function StepIndicator({ step }: { step: 1 | 2 | 3 | 4 }) {
  return (
    <ol aria-label="Setup steps" className="flex flex-wrap gap-x-5 gap-y-1 pb-2.5 pt-1">
      {STEP_LABELS.map((label, index) => {
        const number = index + 1;
        const done = number < step;
        const current = number === step;
        return (
          <li
            key={label}
            aria-current={current ? "step" : undefined}
            className={cn(
              "flex items-center gap-1.5 font-mono text-2xs",
              current ? "text-foreground" : done ? "text-primary" : "text-muted-foreground",
            )}
          >
            {done ? <Check className="size-3" /> : <span>{String(number).padStart(2, "0")}</span>}
            {label}
          </li>
        );
      })}
    </ol>
  );
}

/**
 * Sync progress arrives over the event socket. Until the first progress
 * event the sync has not started, so this never claims "Ready" early.
 */
function FirstSync({
  sync,
  onOpenInbox,
  onManage,
}: {
  sync: { current: number; total: number } | undefined;
  onOpenInbox: () => void;
  onManage: () => void;
}) {
  const [started, setStarted] = useState(false);
  useEffect(() => {
    if (sync) setStarted(true);
  }, [sync]);
  const phase = sync ? "running" : started ? "done" : "waiting";
  const percent =
    phase === "done" ? 100 : sync ? Math.round((sync.current / Math.max(1, sync.total)) * 100) : 0;
  return (
    <section>
      <p className="text-[13px] text-muted-foreground">
        The daemon keeps syncing if you leave this page.
      </p>
      <div className="mt-5 border-y border-border py-5" role="status">
        <div className="text-[15px] font-semibold">
          {phase === "running" && sync
            ? `Syncing: ${sync.current.toLocaleString()} of ${plural(sync.total, "message")}`
            : phase === "done"
              ? "First sync finished"
              : "Waiting for the first sync to start…"}
        </div>
        <div
          className="mt-3 h-1.5 overflow-hidden rounded-full bg-muted"
          role="progressbar"
          aria-label="First sync progress"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={phase === "waiting" ? undefined : percent}
        >
          <div
            className={cn("h-full bg-primary", phase === "waiting" && "w-1/4 animate-pulse")}
            style={phase === "waiting" ? undefined : { width: `${percent}%` }}
          />
        </div>
      </div>
      <div className="mt-4 flex gap-2">
        <Button size="sm" onClick={onOpenInbox}>
          Open inbox
        </Button>
        <Button variant="ghost" size="sm" onClick={onManage}>
          Manage accounts
        </Button>
      </div>
    </section>
  );
}

const providerTiles = [
  { id: "gmail" as const, label: "Gmail", description: "Bundled OAuth when available", Icon: Mail },
  {
    id: "outlook" as const,
    label: "Outlook",
    description: "Personal Microsoft account",
    Icon: Mail,
  },
  {
    id: "imap" as const,
    label: "IMAP/SMTP",
    description: "Bring any standards-based mailbox",
    Icon: Server,
  },
];

function Text({
  label,
  value,
  onChange,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: string;
}) {
  return (
    <div className="space-y-1">
      <Label>{label}</Label>
      <Input type={type} value={value} onChange={(event) => onChange(event.target.value)} />
    </div>
  );
}

function NumberField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="space-y-1">
      <Label>{label}</Label>
      <Input
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </div>
  );
}
