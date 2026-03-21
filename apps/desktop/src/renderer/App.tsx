import { FormEvent, useEffect, useMemo, useState } from "react";
import type { BridgeState } from "../shared/types";

interface MailboxEnvelope {
  id: string;
  thread_id: string;
  subject: string;
  snippet: string;
  from: { name?: string | null; email: string };
  date: string;
}

interface ThreadPayload {
  thread: { id: string; subject: string; snippet: string };
  messages: MailboxEnvelope[];
  bodies: Array<{
    message_id: string;
    text_plain?: string | null;
    text_html?: string | null;
  }>;
}

const steps = [
  "Homebrew: brew upgrade mxr",
  "Release install: rerun ./install.sh",
  "Source install: git pull && cargo install --path crates/daemon --locked",
];

export default function App() {
  const [bridge, setBridge] = useState<BridgeState>({ kind: "idle" });
  const [externalPath, setExternalPath] = useState("");
  const [mailbox, setMailbox] = useState<MailboxEnvelope[]>([]);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [thread, setThread] = useState<ThreadPayload | null>(null);
  const [query, setQuery] = useState("");
  const [latestEvent, setLatestEvent] = useState<string | null>(null);

  useEffect(() => {
    void window.mxrDesktop.getBridgeState().then(setBridge);
  }, []);

  useEffect(() => {
    if (bridge.kind !== "ready") {
      return;
    }

    void fetchJson<{ envelopes: MailboxEnvelope[] }>(
      bridge.baseUrl,
      bridge.authToken,
      "/mailbox",
    ).then((data) => {
      setMailbox(data.envelopes);
      if (!selectedThreadId && data.envelopes[0]) {
        setSelectedThreadId(data.envelopes[0].thread_id);
      }
    });

    const socket = new WebSocket(
      `${bridge.baseUrl.replace(/^http/, "ws")}/events?token=${encodeURIComponent(bridge.authToken)}`,
    );
    socket.onmessage = (event) => setLatestEvent(event.data as string);
    return () => socket.close();
  }, [bridge]);

  useEffect(() => {
    if (bridge.kind !== "ready" || !selectedThreadId) {
      return;
    }

    void fetchJson<ThreadPayload>(
      bridge.baseUrl,
      bridge.authToken,
      `/thread/${selectedThreadId}`,
    ).then(setThread);
  }, [bridge, selectedThreadId]);

  const selectedHtml = useMemo(() => {
    if (!thread) {
      return null;
    }
    const htmlBody = thread.bodies.find((body) => body.text_html)?.text_html;
    if (!htmlBody) {
      return null;
    }
    return htmlBody;
  }, [thread]);

  async function runSearch(event: FormEvent) {
    event.preventDefault();
    if (bridge.kind !== "ready") {
      return;
    }
    const data = await fetchJson<{ results: Array<{ message_id: string; thread_id: string }> }>(
      bridge.baseUrl,
      bridge.authToken,
      `/search?q=${encodeURIComponent(query)}`,
    );
    if (!data.results[0]) {
      return;
    }
    setSelectedThreadId(data.results[0].thread_id);
  }

  async function archiveSelected() {
    if (bridge.kind !== "ready" || !thread?.messages[0]) {
      return;
    }
    await fetchJson<{ ok: boolean }>(bridge.baseUrl, bridge.authToken, "/mutations/archive", {
      method: "POST",
      body: JSON.stringify({ message_ids: [thread.messages[0].id] }),
    });
    const refreshed = await fetchJson<{ envelopes: MailboxEnvelope[] }>(
      bridge.baseUrl,
      bridge.authToken,
      "/mailbox",
    );
    setMailbox(refreshed.envelopes);
    setSelectedThreadId(refreshed.envelopes[0]?.thread_id ?? null);
  }

  if (bridge.kind === "mismatch") {
    return (
      <div className="shell shell-centered">
        <section className="panel mismatch-panel">
          <div className="eyebrow">mxr Desktop</div>
          <h1>mxr Desktop needs a compatible version of mxr</h1>
          <p className="lede">{bridge.detail}</p>
          <div className="stats">
            <div>
              <span>Found daemon version</span>
              <strong>{bridge.daemonVersion ?? "unknown"}</strong>
            </div>
            <div>
              <span>Found protocol</span>
              <strong>{bridge.actualProtocol ?? "unknown"}</strong>
            </div>
            <div>
              <span>Required protocol</span>
              <strong>{bridge.requiredProtocol}</strong>
            </div>
          </div>
          <div className="actions">
            <button onClick={() => void window.mxrDesktop.useBundledMxr().then(setBridge)}>
              Use bundled mxr
            </button>
            <button className="secondary" onClick={() => void window.mxrDesktop.retryBridge().then(setBridge)}>
              Retry
            </button>
          </div>
          <div className="steps">
            <h2>Update steps</h2>
            <ul>
              {bridge.updateSteps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ul>
          </div>
          <form
            className="external-form"
            onSubmit={(event) => {
              event.preventDefault();
              void window.mxrDesktop.setExternalBinaryPath(externalPath).then(setBridge);
            }}
          >
            <label htmlFor="external-binary">Advanced: external mxr binary</label>
            <input
              id="external-binary"
              value={externalPath}
              onChange={(event) => setExternalPath(event.target.value)}
              placeholder="/usr/local/bin/mxr"
            />
            <button className="secondary" type="submit">
              Try external binary
            </button>
          </form>
        </section>
      </div>
    );
  }

  if (bridge.kind === "error") {
    return (
      <div className="shell shell-centered">
        <section className="panel mismatch-panel">
          <div className="eyebrow">mxr Desktop</div>
          <h1>{bridge.title}</h1>
          <p className="lede">{bridge.detail}</p>
          <div className="steps">
            <h2>Useful next steps</h2>
            <ul>
              {steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ul>
          </div>
          <div className="actions">
            <button onClick={() => void window.mxrDesktop.retryBridge().then(setBridge)}>Retry</button>
          </div>
        </section>
      </div>
    );
  }

  if (bridge.kind !== "ready") {
    return (
      <div className="shell shell-centered">
        <section className="panel loading-panel">
          <div className="eyebrow">mxr Desktop</div>
          <h1>Connecting to local mail runtime</h1>
        </section>
      </div>
    );
  }

  return (
    <div className="shell desktop-shell">
      <aside className="sidebar panel">
        <div className="sidebar-header">
          <div>
            <div className="eyebrow">mxr Desktop</div>
            <h1>Mailroom</h1>
          </div>
          <div className="pill">protocol {bridge.protocolVersion}</div>
        </div>
        <form className="search" onSubmit={runSearch}>
          <input
            aria-label="Search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search local mail"
          />
          <button type="submit">Search</button>
        </form>
        <div className="list">
          {mailbox.map((envelope) => (
            <button
              className={`list-row ${selectedThreadId === envelope.thread_id ? "active" : ""}`}
              key={envelope.id}
              onClick={() => setSelectedThreadId(envelope.thread_id)}
            >
              <span className="from">{envelope.from.name || envelope.from.email}</span>
              <strong>{envelope.subject}</strong>
              <p>{envelope.snippet}</p>
            </button>
          ))}
        </div>
      </aside>
      <main className="reader panel">
        <div className="reader-header">
          <div>
            <div className="eyebrow">Thread</div>
            <h2>{thread?.thread.subject ?? "Select a thread"}</h2>
          </div>
          <div className="reader-actions">
            <button onClick={archiveSelected}>Archive</button>
          </div>
        </div>
        {latestEvent ? <p className="event-banner">daemon event: {latestEvent}</p> : null}
        {!thread ? (
          <p className="empty">No thread selected.</p>
        ) : selectedHtml ? (
          <iframe
            className="html-frame"
            sandbox="allow-popups allow-popups-to-escape-sandbox"
            srcDoc={selectedHtml}
            title="Email HTML preview"
          />
        ) : (
          <article className="plain-body">
            {thread.bodies.find((body) => body.text_plain)?.text_plain ?? "No readable body"}
          </article>
        )}
      </main>
    </div>
  );
}

async function fetchJson<T>(
  baseUrl: string,
  authToken: string,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    headers: {
      "content-type": "application/json",
      "x-mxr-bridge-token": authToken,
      ...(init?.headers ?? {}),
    },
    ...init,
  });
  if (!response.ok) {
    throw new Error(`Request failed for ${path}: ${response.status}`);
  }
  return (await response.json()) as T;
}
