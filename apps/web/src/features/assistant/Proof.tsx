/*
 * A change waiting for signature, set out as a proof on the overlay
 * surface: the change in the daemon's words, why, exactly what it touches
 * as rows, and one Apply. Approving runs exactly the call stored in the
 * transcript (D122); the exact call is under "x". The card collapses to a
 * wire line, with Undo where there is one, once the conversation shows it
 * applied.
 */

import { useEffect, useRef, useState, type FormEvent } from "react";

import type { AssistantDecision, AssistantPending } from "./api";
import { onBareKey, PENDING_CARD_ATTRIBUTE } from "./cardKeys";
import { previewDraft, previewMessages, type PreviewMessage } from "./describe";
import { irreversibleWarning } from "./wire";
import { Avatar } from "@/components/Avatar";
import { KeyChip } from "@/components/KeyChip";
import { Row } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { formatWhen } from "@/lib/format";

const PREVIEW_ROWS = 5;
const COMPACT = { "--row-height": "44px" } as React.CSSProperties;

type Approval = Extract<AssistantPending, { kind: "approval" }>;

export function Proof({
  item,
  busy,
  onDecide,
}: {
  item: Approval;
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [note, setNote] = useState("");
  const [showAll, setShowAll] = useState(false);
  const [showCall, setShowCall] = useState(false);
  // A held or doubled key must not decide twice before the conversation answers.
  const decided = useRef(false);
  useEffect(() => {
    // A failed decision leaves the proof to try again.
    if (!busy) decided.current = false;
  }, [busy]);
  const messages = previewMessages(item.preview);
  const draft = messages.length === 0 ? previewDraft(item.preview) : null;
  // The daemon's line; the client's own rule only for a daemon that sends none.
  const warning = item.warning || irreversibleWarning(item.tool, item.arguments);
  const title = item.title || "Change waiting for you";

  const decide = (decision: AssistantDecision) => {
    if (busy || decided.current) return;
    decided.current = true;
    onDecide(decision);
  };
  const apply = () => decide({ call_id: item.call_id, decision: "approve" });
  const sendBack = (event?: FormEvent) => {
    event?.preventDefault();
    decide({ call_id: item.call_id, decision: "reject", reason: note.trim() || undefined });
  };

  const shown = showAll ? messages : messages.slice(0, PREVIEW_ROWS);
  return (
    <section
      aria-label={title}
      tabIndex={-1}
      {...{ [PENDING_CARD_ATTRIBUTE]: "", "data-keys-owner": "" }}
      onKeyDown={onBareKey((key) => {
        if (rejecting) return false;
        if (key === "y") apply();
        else if (key === "n") setRejecting(true);
        else if (key === "x") setShowCall((value) => !value);
        else return false;
        return true;
      })}
      className="border border-rule-soft bg-ink-overlay py-3 outline-none focus-visible:border-signal"
    >
      <div className="space-y-1 px-4">
        <p className="type-eyebrow text-text-3">Proof</p>
        <h3 className="type-heading text-text-1">{title}</h3>
        {item.reason ? <p className="type-body text-text-2">{item.reason}</p> : null}
      </div>

      {messages.length > 0 ? (
        <div className="mt-2">
          <ul aria-label="What this changes" style={COMPACT}>
            {shown.map((message) => (
              <MessageRow key={message.key} message={message} />
            ))}
          </ul>
          {messages.length > PREVIEW_ROWS ? (
            <button
              type="button"
              aria-expanded={showAll}
              onClick={() => setShowAll((value) => !value)}
              className="type-meta px-4 py-1 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1"
            >
              {showAll ? "Show fewer" : `and ${messages.length - PREVIEW_ROWS} more`}
            </button>
          ) : null}
        </div>
      ) : draft ? (
        <div className="mt-3 space-y-1 px-4">
          <p className="type-meta text-text-2">To {draft.to || "(no recipients)"}</p>
          <p className="type-body text-text-1">{draft.subject || "(no subject)"}</p>
          <p className="type-reading line-clamp-6 whitespace-pre-wrap text-text-1">{draft.body}</p>
        </div>
      ) : item.preview_error ? (
        <p className="type-meta mt-2 px-4 text-cue">The preview failed: {item.preview_error}</p>
      ) : null}

      {showCall ? (
        <pre className="type-meta mx-4 mt-3 max-h-40 overflow-auto bg-ink-room p-2 break-all whitespace-pre-wrap text-text-2">
          {item.tool} {JSON.stringify(item.arguments, null, 2)}
          {item.preview && messages.length === 0 && !draft
            ? `\n\n${JSON.stringify(item.preview, null, 2)}`
            : ""}
        </pre>
      ) : null}

      {rejecting ? (
        <form onSubmit={sendBack} className="mt-3 flex items-center gap-2 px-4">
          <input
            autoFocus
            aria-label="Why, or what to do instead"
            value={note}
            onChange={(event) => setNote(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setRejecting(false);
              }
            }}
            placeholder="Why, or what to do instead (optional)"
            className="type-body h-8 min-w-0 flex-1 border border-rule bg-ink-room px-2 text-text-1 outline-none placeholder:text-text-3 focus:border-signal"
          />
          <Button type="submit" size="sm" variant="secondary" disabled={busy}>
            Send back
          </Button>
        </form>
      ) : (
        <div className="mt-3 px-4">
          {warning ? <p className="type-meta mb-2 text-danger">{warning}</p> : null}
          <div className="flex flex-wrap items-center gap-x-1 gap-y-2">
            <QuietKey keys="n" label="send back with a note" onClick={() => setRejecting(true)} />
            <QuietKey
              keys="x"
              label="exact call"
              pressed={showCall}
              onClick={() => setShowCall((value) => !value)}
            />
            <Button size="sm" className="ml-auto" disabled={busy} onClick={apply}>
              Apply
              <KeyChip className="pointer-coarse:hidden border-transparent bg-transparent text-inherit shadow-none">
                y
              </KeyChip>
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

function MessageRow({ message }: { message: PreviewMessage }) {
  return (
    <Row
      lit={false}
      className="mx-0"
      lead={<Avatar name={message.from || message.subject} size="sm" />}
      primary={message.from || "(unknown sender)"}
      secondary={message.subject}
      time={message.date ? formatWhen(message.date) : undefined}
    />
  );
}

export function QuietKey({
  keys,
  label,
  onClick,
  pressed,
}: {
  keys: string;
  label: string;
  onClick: () => void;
  pressed?: boolean;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className="type-meta inline-flex h-8 items-center gap-1.5 px-1.5 whitespace-nowrap text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1"
    >
      <KeyChip className="h-4 px-1 pointer-coarse:hidden">{keys}</KeyChip>
      {label}
    </button>
  );
}
