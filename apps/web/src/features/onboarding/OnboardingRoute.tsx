import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { CheckCircle2, Mail, Server, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import {
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
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
    smtpHost: "",
    smtpPort: 587,
    username: "",
    password: "",
  });

  const authSession = useQuery({
    queryKey: ["auth-session", sessionId],
    queryFn: () => fetchAuthSession(sessionId ?? ""),
    enabled: Boolean(sessionId),
    refetchInterval: (query) => (query.state.data?.session.state === "authorized" ? false : 1500),
  });
  const startAuth = useMutation({
    mutationFn: (account: AccountConfig) => startAuthSession(account),
    onSuccess: (result) => {
      setSessionId(result.session.session_id);
      setStep(3);
    },
    onError: (error) => toast.error("OAuth start failed", { description: error.message }),
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

  return (
    <div className="flex min-w-0 flex-1 items-center justify-center overflow-auto bg-background p-6">
      <div className="w-full max-w-3xl rounded-2xl border border-border bg-surface-elevated p-6 shadow-2xl">
        <div className="mb-6 flex items-center gap-2 font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          {[1, 2, 3, 4].map((item) => (
            <span key={item} className={item <= step ? "text-primary" : undefined}>
              0{item}
            </span>
          ))}
        </div>
        {step === 1 ? (
          <section className="grid gap-6 md:grid-cols-[1fr_260px]">
            <div>
              <h1 className="text-3xl font-semibold tracking-tight">Bring your mailbox local.</h1>
              <p className="mt-3 text-sm text-muted-foreground">
                mxr keeps SQLite as truth, syncs in the daemon, and makes search fast before
                anything else. Start with your mailbox, or try <code>mxr demo</code> for a
                two-account inbox with threads, attachments, newsletters, promos, spam, rules, and
                analytics.
              </p>
              <Button className="mt-6" onClick={() => setStep(2)}>
                Connect first account
              </Button>
            </div>
            <div className="rounded-xl border border-border bg-background p-4">
              <ShieldCheck className="mb-3 size-6 text-primary" />
              <div className="text-sm font-medium">A real workflow, not a blank slate</div>
              <p className="mt-2 text-xs text-muted-foreground">
                The demo seed is built to exercise search, sender profiles, LLM summaries,
                unsubscribe, attachments, links, images, repeat contacts, suspicious mail, and
                prewarmed analytics.
              </p>
            </div>
          </section>
        ) : null}
        {step === 2 ? (
          <section>
            <h1 className="text-xl font-semibold">Choose provider</h1>
            <div className="mt-4 grid gap-3 md:grid-cols-3">
              {providerTiles.map((tile) => (
                <button
                  key={tile.id}
                  className={`rounded-xl border p-4 text-left ${provider === tile.id ? "border-primary bg-primary-muted" : "border-border bg-background"}`}
                  onClick={() => setProvider(tile.id)}
                >
                  <tile.Icon className="mb-3 size-5 text-primary" />
                  <div className="text-sm font-medium">{tile.label}</div>
                  <div className="mt-1 text-2xs text-muted-foreground">{tile.description}</div>
                </button>
              ))}
            </div>
            <div className="mt-5 space-y-2">
              <Label>Email</Label>
              <Input
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@example.com"
              />
            </div>
            <Button
              className="mt-4"
              disabled={!email.trim()}
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
            <h1 className="text-xl font-semibold">Authorize {provider}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              Use the device code flow. mxr never needs a redirect callback in the browser.
            </p>
            <div className="mt-5 rounded-xl border border-border bg-background p-5">
              <div className="font-mono text-3xl tracking-widest text-primary">
                {session?.user_code ?? "..."}
              </div>
              <div
                className={`mt-2 whitespace-pre-wrap text-xs ${
                  session?.error ? "text-destructive" : "text-muted-foreground"
                }`}
              >
                {session?.error ??
                  session?.message ??
                  session?.state ??
                  "Waiting for authorization"}
              </div>
              {session?.verification_uri || session?.auth_url ? (
                <Button
                  className="mt-4"
                  onClick={() =>
                    window.open(
                      session.verification_uri ?? session.auth_url,
                      "_blank",
                      "noopener,noreferrer",
                    )
                  }
                >
                  Open sign-in
                </Button>
              ) : null}
            </div>
            <div className="mt-4 flex gap-2">
              <Button
                disabled={session?.state !== "authorized" || completeAuth.isPending}
                onClick={() => completeAuth.mutate()}
              >
                <CheckCircle2 className="size-3" />
                Complete
              </Button>
              <Button variant="ghost" onClick={() => setStep(2)}>
                Back
              </Button>
            </div>
          </section>
        ) : null}
        {step === 3 && provider === "imap" ? (
          <section>
            <h1 className="text-xl font-semibold">IMAP + SMTP</h1>
            <div className="mt-4 grid gap-3 md:grid-cols-2">
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
            <Button
              className="mt-4"
              disabled={saveImap.isPending || !imap.email || !imap.imapHost || !imap.smtpHost}
              onClick={() => saveImap.mutate()}
            >
              Test and save
            </Button>
          </section>
        ) : null}
        {step === 4 ? (
          <section>
            <h1 className="text-xl font-semibold">Initial sync</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              The daemon keeps syncing even if you leave this page.
            </p>
            <div className="mt-5 rounded-xl border border-border bg-background p-5">
              <div className="text-3xl font-semibold">
                {sync ? `${sync.current}/${sync.total}` : "Ready"}
              </div>
              <div className="mt-2 h-2 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full bg-primary"
                  style={{
                    width: sync
                      ? `${Math.round((sync.current / Math.max(1, sync.total)) * 100)}%`
                      : "100%",
                  }}
                />
              </div>
            </div>
            <div className="mt-4 flex gap-2">
              <Button onClick={() => navigate({ to: "/m/$mailbox", params: { mailbox: "inbox" } })}>
                Open inbox
              </Button>
              <Button variant="ghost" onClick={() => navigate({ to: "/accounts" })}>
                Manage accounts
              </Button>
            </div>
          </section>
        ) : null}
      </div>
    </div>
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
