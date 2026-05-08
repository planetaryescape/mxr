import { Dialog } from "@base-ui/react";
import { useEffect, useState } from "react";
import type { components } from "../../shared/api.generated";
import type { BridgeReadyState } from "../../shared/types";
import { fetchJson } from "../state/bridgeHttp";
import { cn } from "../lib/cn";

type SnippetData = components["schemas"]["SnippetData"];
type SenderProfileData = components["schemas"]["SenderProfileData"];
type ScreenerQueueEntry = components["schemas"]["ScreenerQueueEntryData"];
type ScreenerDispositionData = components["schemas"]["ScreenerDispositionData"];

// Generic Envelope shape — only the fields the dialogs render. The
// generated `components["schemas"]["Envelope"]` carries broader fields
// we don't need; subset narrows the render contract.
type EnvelopeRow = {
  id: string;
  subject: string;
  snippet: string;
  date: string;
  from: { name?: string | null; email: string };
};

type EnvelopeFromBridge = components["schemas"]["Envelope"];

const BRIDGE_BASE_PATH = "/api/v1/mail";

/// Discriminated union for which browser dialog is open. `kind: null`
/// means no dialog is open. The kinded variants carry whatever context
/// the dialog needs (e.g., the sender email for the profile lookup).
export type BrowserDialogState =
  | { kind: "snippets" }
  | { kind: "reply_queue" }
  | { kind: "screener"; accountId: string }
  | { kind: "sender"; accountId: string; email: string }
  | { kind: "summary"; threadId: string }
  | { kind: "draft_assist"; threadId: string }
  | null;

export function BrowserDialogs(props: {
  bridge: BridgeReadyState | null;
  state: BrowserDialogState;
  onClose: () => void;
  onShowNotice: (message: string) => void;
}) {
  if (!props.bridge || !props.state) {
    return null;
  }
  switch (props.state.kind) {
    case "snippets":
      return (
        <SnippetsBrowserDialog bridge={props.bridge} onClose={props.onClose} />
      );
    case "reply_queue":
      return (
        <ReplyQueueBrowserDialog
          bridge={props.bridge}
          onClose={props.onClose}
        />
      );
    case "sender":
      return (
        <SenderProfileBrowserDialog
          bridge={props.bridge}
          accountId={props.state.accountId}
          email={props.state.email}
          onClose={props.onClose}
        />
      );
    case "summary":
      return (
        <SummaryBrowserDialog
          bridge={props.bridge}
          threadId={props.state.threadId}
          onClose={props.onClose}
        />
      );
    case "draft_assist":
      return (
        <DraftAssistBrowserDialog
          bridge={props.bridge}
          threadId={props.state.threadId}
          onClose={props.onClose}
          onShowNotice={props.onShowNotice}
        />
      );
    case "screener":
      return (
        <ScreenerBrowserDialog
          bridge={props.bridge}
          accountId={props.state.accountId}
          onClose={props.onClose}
          onShowNotice={props.onShowNotice}
        />
      );
  }
}

function ModalShell(props: {
  open: boolean;
  title: string;
  helper?: string;
  width?: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup
          className={cn(
            "fixed left-1/2 top-1/2 z-40 -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none",
            props.width ?? "w-[min(48rem,calc(100vw-3rem))]",
          )}
        >
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            {props.title}
          </Dialog.Title>
          {props.helper ? (
            <Dialog.Description className="mt-3 text-sm text-foreground-muted">
              {props.helper}
            </Dialog.Description>
          ) : null}
          {props.children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function LoadingPlaceholder({ label }: { label: string }) {
  return (
    <div className="mt-6 rounded border border-outline bg-panel-elevated px-5 py-8 text-center text-sm text-foreground-muted">
      {label}
    </div>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="mt-6 rounded border border-danger/30 bg-danger/10 px-5 py-4 text-sm text-danger">
      {message}
    </div>
  );
}

function EmptyHint(props: { title: string; cli: string; body: string }) {
  return (
    <div className="mt-6 rounded border border-outline bg-panel-elevated px-5 py-8 text-sm">
      <p className="text-foreground">{props.title}</p>
      <p className="mt-3 text-foreground-muted">{props.body}</p>
      <p className="mt-4 font-mono text-[12px] text-foreground-subtle">
        {props.cli}
      </p>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*                              Snippets browser                              */
/* -------------------------------------------------------------------------- */

export function SnippetsBrowserDialog(props: {
  bridge: BridgeReadyState;
  onClose: () => void;
}) {
  const { snippets, loading, error } = useBridgeQuery<SnippetData[]>({
    bridge: props.bridge,
    path: `${BRIDGE_BASE_PATH}/snippets`,
    label: "snippets:list",
    selector: (data: { kind: string; snippets?: SnippetData[] }) =>
      data.kind === "Snippets" ? data.snippets ?? [] : [],
  });
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    setSelectedIndex(0);
  }, [snippets]);

  const selected = snippets?.[selectedIndex] ?? null;

  return (
    <ModalShell
      open
      title="Snippets"
      helper="Read-only browser. Use `mxr snippets set` / `remove` to edit."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      {loading ? <LoadingPlaceholder label="Loading snippets..." /> : null}
      {!loading && !error && snippets && snippets.length === 0 ? (
        <EmptyHint
          title="No snippets yet."
          body="Snippets keep your usual replies one keystroke away."
          cli="mxr snippets set thanks --body 'Thanks for reaching out!'"
        />
      ) : null}
      {!loading && !error && snippets && snippets.length > 0 ? (
        <div className="mt-5 grid gap-4 md:grid-cols-[14rem_1fr]">
          <ul
            role="listbox"
            aria-label="Snippets"
            className="subtle-scrollbar max-h-[60vh] overflow-y-auto rounded border border-outline bg-canvas-elevated"
          >
            {snippets.map((snippet, index) => (
              <li key={snippet.name}>
                <button
                  type="button"
                  role="option"
                  aria-selected={index === selectedIndex}
                  className={cn(
                    "block w-full px-3 py-2 text-left text-sm",
                    index === selectedIndex
                      ? "bg-accent/10 font-medium text-foreground"
                      : "text-foreground-muted hover:bg-panel-elevated/60",
                  )}
                  onClick={() => setSelectedIndex(index)}
                >
                  {snippet.name}
                </button>
              </li>
            ))}
          </ul>
          <section className="rounded border border-outline bg-panel-muted px-4 py-4">
            {selected ? (
              <>
                {selected.vars && selected.vars.length > 0 ? (
                  <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
                    Vars: {selected.vars.join(", ")}
                  </p>
                ) : null}
                <pre className="mt-3 whitespace-pre-wrap font-mono text-sm leading-7 text-foreground">
                  {selected.body}
                </pre>
              </>
            ) : null}
          </section>
        </div>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

/* -------------------------------------------------------------------------- */
/*                              Reply-later queue                             */
/* -------------------------------------------------------------------------- */

export function ReplyQueueBrowserDialog(props: {
  bridge: BridgeReadyState;
  onClose: () => void;
}) {
  const { messages, loading, error } = useBridgeQuery<EnvelopeRow[]>({
    bridge: props.bridge,
    path: `${BRIDGE_BASE_PATH}/reply-later`,
    label: "reply-later:list",
    selector: (
      data: { kind: string; messages?: EnvelopeFromBridge[] },
    ) =>
      data.kind === "ReplyQueue"
        ? (data.messages ?? []).map(envelopeToRow)
        : [],
  }, "messages");
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    setSelectedIndex(0);
  }, [messages]);

  const selected = messages?.[selectedIndex] ?? null;

  return (
    <ModalShell
      open
      title="Reply Later"
      helper="Use `b` while reading a message to flag it for this queue."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      {loading ? <LoadingPlaceholder label="Loading reply queue..." /> : null}
      {!loading && !error && messages && messages.length === 0 ? (
        <EmptyHint
          title="Reply queue is empty."
          body="Flag messages with `b` while reading to add them here."
          cli="mxr replies"
        />
      ) : null}
      {!loading && !error && messages && messages.length > 0 ? (
        <div className="mt-5 grid gap-4 md:grid-cols-[16rem_1fr]">
          <ul
            role="listbox"
            aria-label="Flagged messages"
            className="subtle-scrollbar max-h-[60vh] overflow-y-auto rounded border border-outline bg-canvas-elevated"
          >
            {messages.map((envelope, index) => (
              <li key={envelope.id}>
                <button
                  type="button"
                  role="option"
                  aria-selected={index === selectedIndex}
                  className={cn(
                    "block w-full px-3 py-2 text-left text-sm",
                    index === selectedIndex
                      ? "bg-accent/10 font-medium text-foreground"
                      : "text-foreground-muted hover:bg-panel-elevated/60",
                  )}
                  onClick={() => setSelectedIndex(index)}
                >
                  <span className="block truncate">
                    {envelope.from.name ?? envelope.from.email}
                  </span>
                  <span className="mt-0.5 block truncate text-foreground-subtle">
                    {envelope.subject || "(no subject)"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          <section className="rounded border border-outline bg-panel-muted px-4 py-4 text-sm">
            {selected ? (
              <>
                <DefField label="Subject" value={selected.subject || "(no subject)"} />
                <DefField label="From" value={selected.from.email} />
                <DefField label="Date" value={selected.date} />
                <p className="mt-4 whitespace-pre-wrap text-foreground-muted">
                  {selected.snippet}
                </p>
              </>
            ) : null}
          </section>
        </div>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

/* -------------------------------------------------------------------------- */
/*                             Sender profile view                            */
/* -------------------------------------------------------------------------- */

export function SenderProfileBrowserDialog(props: {
  bridge: BridgeReadyState;
  accountId: string;
  email: string;
  onClose: () => void;
}) {
  const path = `${BRIDGE_BASE_PATH}/sender?account_id=${encodeURIComponent(
    props.accountId,
  )}&email=${encodeURIComponent(props.email)}`;
  const { profile, loading, error } = useBridgeQuery<SenderProfileData | null>({
    bridge: props.bridge,
    path,
    label: "sender:profile",
    selector: (data: { kind: string; profile?: SenderProfileData | null }) =>
      data.kind === "SenderProfile" ? data.profile ?? null : null,
  }, "profile");

  return (
    <ModalShell
      open
      title={`Sender · ${props.email}`}
      helper="Aggregates pulled from the local contacts table."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      {loading ? <LoadingPlaceholder label="Loading sender profile..." /> : null}
      {!loading && !error && !profile ? (
        <EmptyHint
          title="Sender unknown."
          body="No contact data yet. Run a sync to populate the contacts table."
          cli={`mxr sender ${props.email}`}
        />
      ) : null}
      {!loading && !error && profile ? (
        <div className="mt-5 grid gap-3 text-sm">
          {profile.display_name ? (
            <DefField label="Name" value={profile.display_name} />
          ) : null}
          <DefField
            label="Volume"
            value={`${profile.total_inbound} inbound · ${profile.total_outbound} outbound · ${profile.replied_count} replied`}
          />
          {profile.cadence_days_p50 != null ? (
            <DefField
              label="Cadence p50"
              value={`${profile.cadence_days_p50.toFixed(1)} days`}
            />
          ) : null}
          <DefField
            label="Open threads"
            value={String(profile.open_thread_count)}
          />
          <DefField
            label="First seen"
            value={shortDate(profile.first_seen_at)}
          />
          <DefField
            label="Last seen"
            value={shortDate(profile.last_seen_at)}
          />
          {profile.last_inbound_at ? (
            <DefField
              label="Last from"
              value={shortDate(profile.last_inbound_at)}
            />
          ) : null}
          {profile.last_outbound_at ? (
            <DefField
              label="Last to"
              value={shortDate(profile.last_outbound_at)}
            />
          ) : null}
          {profile.is_list_sender ? (
            <p className="mt-2 rounded border border-warning/30 bg-warning/10 px-3 py-2 text-foreground">
              List sender — List-ID: {profile.list_id ?? "(none)"}
            </p>
          ) : null}
        </div>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

/* -------------------------------------------------------------------------- */
/*                              Screener queue                                */
/* -------------------------------------------------------------------------- */

const DISPOSITIONS: Array<{
  key: ScreenerDispositionData;
  label: string;
  shortcut: string;
}> = [
  { key: "allow", label: "Allow", shortcut: "a" },
  { key: "deny", label: "Deny", shortcut: "d" },
  { key: "feed", label: "Feed", shortcut: "f" },
  { key: "paper_trail", label: "Paper Trail", shortcut: "p" },
];

export function ScreenerBrowserDialog(props: {
  bridge: BridgeReadyState;
  accountId: string;
  onClose: () => void;
  onShowNotice: (message: string) => void;
}) {
  const path = `${BRIDGE_BASE_PATH}/screener/queue?account_id=${encodeURIComponent(props.accountId)}`;
  const [refreshKey, setRefreshKey] = useState(0);
  const [pending, setPending] = useState<string | null>(null);
  const { entries, loading, error } = useBridgeQuery<ScreenerQueueEntry[]>(
    {
      bridge: props.bridge,
      path,
      label: "screener:queue",
      selector: (data: { kind: string; entries?: ScreenerQueueEntry[] }) =>
        data.kind === "ScreenerQueue" ? data.entries ?? [] : [],
    },
    "entries",
    refreshKey,
  );
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    setSelectedIndex(0);
  }, [entries]);

  const selected = entries?.[selectedIndex] ?? null;

  async function dispose(disposition: ScreenerDispositionData) {
    if (!selected) {
      return;
    }
    const senderEmail = selected.sender_email;
    setPending(senderEmail);
    try {
      await fetchJson<unknown>(
        props.bridge.baseUrl,
        props.bridge.authToken,
        `${BRIDGE_BASE_PATH}/screener/decisions`,
        {
          method: "POST",
          body: JSON.stringify({
            account_id: props.accountId,
            sender_email: senderEmail,
            disposition,
            route_label: null,
          }),
          requestLabel: "screener:dispose",
        },
      );
      props.onShowNotice(
        `Screener: ${disposition.replace("_", " ")} ${senderEmail}`,
      );
      setRefreshKey((current) => current + 1);
    } catch (cause) {
      props.onShowNotice(
        `Failed to set screener disposition: ${(cause as Error).message ?? "unknown"}`,
      );
    } finally {
      setPending(null);
    }
  }

  return (
    <ModalShell
      open
      title="Screener"
      helper="Triage senders awaiting consent. Use a/d/f/p for allow / deny / feed / paper-trail."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      {loading ? <LoadingPlaceholder label="Loading screener queue..." /> : null}
      {!loading && !error && entries && entries.length === 0 ? (
        <EmptyHint
          title="Screener queue is empty."
          body="New senders awaiting consent will appear here as they arrive."
          cli={`mxr screener queue --account-id ${props.accountId}`}
        />
      ) : null}
      {!loading && !error && entries && entries.length > 0 ? (
        <div className="mt-5 grid gap-4 md:grid-cols-[16rem_1fr]">
          <ul
            role="listbox"
            aria-label="Senders awaiting decision"
            className="subtle-scrollbar max-h-[55vh] overflow-y-auto rounded border border-outline bg-canvas-elevated"
          >
            {entries.map((entry, index) => (
              <li key={entry.sender_email}>
                <button
                  type="button"
                  role="option"
                  aria-selected={index === selectedIndex}
                  className={cn(
                    "block w-full px-3 py-2 text-left text-sm",
                    index === selectedIndex
                      ? "bg-accent/10 font-medium text-foreground"
                      : "text-foreground-muted hover:bg-panel-elevated/60",
                  )}
                  onClick={() => setSelectedIndex(index)}
                >
                  <span className="block truncate">
                    {entry.display_name ?? entry.sender_email}
                  </span>
                  <span className="mt-0.5 block truncate text-foreground-subtle">
                    {entry.message_count} message
                    {entry.message_count === 1 ? "" : "s"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          <section className="rounded border border-outline bg-panel-muted px-4 py-4 text-sm">
            {selected ? (
              <>
                <DefField label="Email" value={selected.sender_email} />
                {selected.display_name ? (
                  <DefField label="Name" value={selected.display_name} />
                ) : null}
                <DefField label="Latest" value={selected.latest_subject} />
                <DefField label="Latest at" value={shortDate(selected.latest_at)} />
                <DefField
                  label="Messages"
                  value={String(selected.message_count)}
                />
                <div className="mt-5 flex flex-wrap gap-2">
                  {DISPOSITIONS.map((item) => (
                    <button
                      key={item.key}
                      type="button"
                      disabled={pending === selected.sender_email}
                      className={cn(
                        "rounded border px-3 py-2 text-sm",
                        pending === selected.sender_email
                          ? "border-outline bg-panel-elevated text-foreground-subtle"
                          : "border-accent/30 bg-accent/12 text-accent hover:bg-accent/20",
                      )}
                      onClick={() => void dispose(item.key)}
                    >
                      {item.label}{" "}
                      <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-foreground-subtle">
                        {item.shortcut}
                      </span>
                    </button>
                  ))}
                </div>
              </>
            ) : null}
          </section>
        </div>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

/* -------------------------------------------------------------------------- */
/*                                   Helpers                                  */
/* -------------------------------------------------------------------------- */

function CloseFooter({ onClose }: { onClose: () => void }) {
  return (
    <div className="mt-6 flex justify-end">
      <button
        type="button"
        className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
        onClick={onClose}
      >
        Close
      </button>
    </div>
  );
}

function DefField({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[7rem_1fr] gap-2 py-1">
      <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
        {label}
      </span>
      <span className="text-foreground">{value}</span>
    </div>
  );
}

function envelopeToRow(envelope: EnvelopeFromBridge): EnvelopeRow {
  return {
    id: envelope.id,
    subject: envelope.subject,
    snippet: envelope.snippet,
    date: envelope.date,
    from: { name: envelope.from.name, email: envelope.from.email },
  };
}

function shortDate(iso: string): string {
  // Slice the leading "YYYY-MM-DD HH:MM" portion of the ISO timestamp;
  // good enough for at-a-glance dialog rendering, no locale baggage.
  const t = iso.indexOf("T");
  if (t < 0) {
    return iso;
  }
  const datePart = iso.slice(0, t);
  const timePart = iso.slice(t + 1, t + 6);
  return `${datePart} ${timePart}`;
}

type Selector<TIn, TOut> = (data: TIn) => TOut;

function useBridgeQuery<TOut>(
  args: {
    bridge: BridgeReadyState;
    path: string;
    label: string;
    selector: Selector<{ kind: string } & Record<string, unknown>, TOut>;
  },
  outputKey?: string,
  refreshKey?: number,
): {
  loading: boolean;
  error: string | null;
  // The selected payload, keyed dynamically. We expose ergonomic
  // getters (snippets / messages / entries / profile) via Proxy-like
  // alias so callsites can destructure intuitively.
  data: TOut | null;
  snippets?: TOut;
  messages?: TOut;
  entries?: TOut;
  profile?: TOut;
} {
  const [data, setData] = useState<TOut | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setData(null);
    fetchJson<{ kind: string } & Record<string, unknown>>(
      args.bridge.baseUrl,
      args.bridge.authToken,
      args.path,
      { requestLabel: args.label },
    )
      .then((payload) => {
        if (cancelled) {
          return;
        }
        try {
          setData(args.selector(payload));
        } catch (cause) {
          setError((cause as Error).message ?? "Unexpected payload");
        }
        setLoading(false);
      })
      .catch((cause: Error) => {
        if (cancelled) {
          return;
        }
        setError(cause.message ?? "Request failed");
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [args.bridge.baseUrl, args.bridge.authToken, args.path, refreshKey]);

  const result = { loading, error, data } as ReturnType<typeof useBridgeQuery<TOut>>;
  if (outputKey) {
    (result as Record<string, unknown>)[outputKey] = data ?? undefined;
  } else {
    // Default export key is "snippets" — matches the most-common use site.
    (result as Record<string, unknown>).snippets = data ?? undefined;
  }
  return result;
}

/* -------------------------------------------------------------------------- */
/*                                 Summary                                    */
/* -------------------------------------------------------------------------- */

export function SummaryBrowserDialog(props: {
  bridge: BridgeReadyState;
  threadId: string;
  onClose: () => void;
}) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<{ text: string; model: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setSummary(null);
    fetchJson<{ kind: string; text?: string; model?: string }>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      `${BRIDGE_BASE_PATH}/threads/${encodeURIComponent(props.threadId)}/summarize`,
      { method: "POST", requestLabel: "summarize" },
    )
      .then((payload) => {
        if (cancelled) return;
        if (payload.kind !== "ThreadSummary" || !payload.text) {
          setError("Unexpected response shape");
        } else {
          setSummary({ text: payload.text, model: payload.model ?? "unknown" });
        }
        setLoading(false);
      })
      .catch((cause: Error) => {
        if (cancelled) return;
        setError(cause.message ?? "Request failed");
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [props.bridge.baseUrl, props.bridge.authToken, props.threadId]);

  const title = summary
    ? `Thread summary · ${summary.model}`
    : "Thread summary";

  return (
    <ModalShell
      open
      title={title}
      helper="LLM-generated 2-3 sentence summary of the thread."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      {loading ? <LoadingPlaceholder label="Summarizing thread..." /> : null}
      {!loading && !error && summary ? (
        <p className="mt-5 whitespace-pre-wrap text-sm leading-7 text-foreground">
          {summary.text}
        </p>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

/* -------------------------------------------------------------------------- */
/*                              Draft assist                                  */
/* -------------------------------------------------------------------------- */

export function DraftAssistBrowserDialog(props: {
  bridge: BridgeReadyState;
  threadId: string;
  onClose: () => void;
  onShowNotice: (message: string) => void;
}) {
  const [instruction, setInstruction] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<{ body: string; model: string } | null>(null);

  async function generate() {
    const trimmed = instruction.trim();
    if (trimmed.length === 0) {
      setError("Type an instruction (e.g., 'decline politely').");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const payload = await fetchJson<{ kind: string; body?: string; model?: string }>(
        props.bridge.baseUrl,
        props.bridge.authToken,
        `${BRIDGE_BASE_PATH}/threads/draft-assist`,
        {
          method: "POST",
          body: JSON.stringify({ thread_id: props.threadId, instruction: trimmed }),
          requestLabel: "draft-assist",
        },
      );
      if (payload.kind !== "DraftSuggestion" || !payload.body) {
        setError("Unexpected response shape");
      } else {
        setDraft({ body: payload.body, model: payload.model ?? "unknown" });
      }
    } catch (cause) {
      setError((cause as Error).message ?? "Request failed");
    } finally {
      setBusy(false);
    }
  }

  async function copyToClipboard() {
    if (!draft) return;
    try {
      await navigator.clipboard.writeText(draft.body);
      props.onShowNotice("Draft copied — paste into compose to review");
    } catch (cause) {
      props.onShowNotice(`Copy failed: ${(cause as Error).message ?? "unknown"}`);
    }
  }

  return (
    <ModalShell
      open
      title={draft ? `Draft assist · ${draft.model}` : "Draft assist"}
      helper="Grounded on your prior sent messages. Always opens for review — never auto-sent."
      onClose={props.onClose}
    >
      {error ? <ErrorBanner message={error} /> : null}
      <label className="mt-5 grid gap-2">
        <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
          Instruction
        </span>
        <textarea
          className="min-h-20 rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
          value={instruction}
          placeholder="e.g., decline politely, suggest next month"
          onChange={(event) => setInstruction(event.target.value)}
          disabled={busy}
        />
      </label>
      <div className="mt-4 flex justify-end">
        <button
          type="button"
          className={cn(
            "rounded border px-4 py-2 text-sm",
            busy
              ? "border-outline bg-panel-elevated text-foreground-subtle"
              : "border-accent/30 bg-accent/12 text-accent hover:bg-accent/20",
          )}
          disabled={busy}
          onClick={() => void generate()}
        >
          {busy ? "Generating..." : draft ? "Regenerate" : "Generate"}
        </button>
      </div>
      {draft ? (
        <div className="mt-5 rounded border border-outline bg-panel-muted px-4 py-4">
          <pre className="whitespace-pre-wrap font-mono text-sm leading-7 text-foreground">
            {draft.body}
          </pre>
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={() => void copyToClipboard()}
            >
              Copy to clipboard
            </button>
          </div>
        </div>
      ) : null}
      <CloseFooter onClose={props.onClose} />
    </ModalShell>
  );
}

