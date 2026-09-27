/*
 * What matters before the messages: the model's gist and the ask on top,
 * then how you know this person, whether you owe a reply, and open promises
 * both ways. Facts come from the store and are there when the thread opens.
 * The model's part has a fixed-height slot, reserved only when a model is
 * configured, so nothing below moves when it arrives; it fades in.
 */

import { Check, RefreshCw } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";

import { cn } from "@/lib/utils";

import type { ThreadContext, ThreadGist } from "./api";
import { contextFacts, firstName, promiseViews, provenanceLabel } from "./contextFormat";

export interface GistState {
  /** A model is configured, so the slot is reserved. */
  reserved: boolean;
  data?: ThreadGist;
  loading: boolean;
  error?: string | null;
  retry: () => void;
}

interface ContextBlockProps {
  context?: ThreadContext;
  gist: GistState;
  onRevealAsk: () => void;
  onResolvePromise: (commitmentId: string) => void;
  resolving: boolean;
}

export function ContextBlock({
  context,
  gist,
  onRevealAsk,
  onResolvePromise,
  resolving,
}: ContextBlockProps) {
  const person = context?.counterparty ?? null;
  const facts = context ? contextFacts(context) : [];
  const promises = context ? promiseViews(context) : [];
  if (!gist.reserved && facts.length === 0 && promises.length === 0) return null;

  return (
    <section
      aria-label="Context"
      data-testid="thread-context"
      className="mx-5 mb-1 mt-4 max-w-[var(--reading-measure)] border-l border-primary/50 pl-4"
    >
      {gist.reserved ? (
        <GistSlot
          gist={gist}
          personName={person ? firstName(person) : null}
          onRevealAsk={onRevealAsk}
        />
      ) : null}
      {facts.length > 0 ? (
        <p
          data-testid="thread-context-facts"
          className="text-pretty text-[12.5px] leading-5 text-muted-foreground tabular-nums"
        >
          {facts.map((part, index) => (
            <span key={part}>
              {index > 0 ? <span aria-hidden> · </span> : null}
              {part}
            </span>
          ))}
        </p>
      ) : null}
      {promises.length > 0 ? (
        <ul aria-label="Open promises" className="mt-1 space-y-0.5">
          {promises.map((promise) => (
            <li
              key={promise.id}
              className="group flex items-start gap-1.5 text-[12.5px] leading-5 text-foreground/90"
            >
              <span className="min-w-0 text-pretty">{promise.text}</span>
              <button
                type="button"
                disabled={resolving}
                onClick={() => onResolvePromise(promise.id)}
                aria-label={`Mark done: ${promise.text}`}
                title="Mark done"
                className="mt-0.5 shrink-0 rounded text-muted-foreground opacity-60 hover:text-foreground hover:opacity-100 focus-visible:opacity-100 disabled:opacity-30"
              >
                <Check className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

/*
 * Fixed height: an eyebrow line, two lines of gist (three in a narrow
 * reader) and two of ask. Longer text is clamped behind "Show all", the
 * user's own action, so expanding it is not a layout shift.
 */
function GistSlot({
  gist,
  personName,
  onRevealAsk,
}: {
  gist: GistState;
  personName: string | null;
  onRevealAsk: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [clamped, setClamped] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const data = gist.data;
  const ready = data?.status === "ready" && Boolean(data.gist);

  useLayoutEffect(() => {
    const node = bodyRef.current;
    if (!node || expanded) return;
    setClamped(
      Array.from(node.querySelectorAll<HTMLElement>("[data-clamp]")).some(
        (line) => line.scrollHeight > line.clientHeight + 1,
      ),
    );
  }, [data, expanded]);

  return (
    <div
      data-testid="thread-gist"
      data-state={ready ? "ready" : gist.loading ? "loading" : "unavailable"}
      className={cn(
        "mb-2",
        expanded ? "min-h-[9rem] @xl:min-h-[7.5rem]" : "h-[9rem] overflow-hidden @xl:h-[7.5rem]",
      )}
    >
      <p className="flex h-4 items-center gap-3 font-mono text-2xs leading-4 text-muted-foreground">
        <span className="min-w-0 truncate" data-testid="thread-gist-source">
          {ready && data?.provenance
            ? provenanceLabel(data.provenance, personName)
            : gist.loading
              ? "Reading this conversation…"
              : "No gist this time"}
        </span>
        {clamped || expanded ? (
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded((value) => !value)}
            className="shrink-0 text-primary hover:underline"
          >
            {expanded ? "Show less" : "Show all"}
          </button>
        ) : null}
      </p>
      <div
        ref={bodyRef}
        key={ready ? "ready" : "waiting"}
        className="mt-1 animate-in fade-in-0 duration-base ease-out"
      >
        {ready && data ? (
          <>
            <p
              data-clamp
              data-testid="thread-gist-text"
              className={cn(
                "text-pretty text-[15px] leading-6 text-foreground",
                !expanded && "line-clamp-3 @xl:line-clamp-2",
              )}
            >
              {data.gist}
            </p>
            <p
              data-clamp
              data-testid="thread-ask"
              className={cn(
                "mt-1.5 text-pretty text-[13.5px] leading-[1.375rem]",
                !expanded && "line-clamp-2",
              )}
            >
              {data.ask ? (
                <>
                  <span className="font-medium text-warning">Asks you to</span>{" "}
                  <span className="text-foreground">{data.ask.summary}</span>
                  {data.ask.quote ? (
                    <>
                      {" "}
                      <button
                        type="button"
                        onClick={onRevealAsk}
                        className="whitespace-nowrap text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary"
                      >
                        Show in message
                      </button>
                    </>
                  ) : null}
                </>
              ) : (
                <span className="text-muted-foreground">Nothing asked of you.</span>
              )}
            </p>
          </>
        ) : !gist.loading ? (
          <p className="text-pretty text-[13.5px] leading-[1.375rem] text-muted-foreground">
            {unavailableCopy(data, gist.error)}{" "}
            {data?.status !== "blocked" ? (
              <button
                type="button"
                onClick={gist.retry}
                className="inline-flex items-center gap-1 text-primary hover:underline"
              >
                <RefreshCw className="size-3" /> Try again
              </button>
            ) : null}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function unavailableCopy(data: ThreadGist | undefined, error?: string | null): string {
  if (data?.status === "blocked") {
    return "Privacy settings keep this conversation from the configured model.";
  }
  if (data?.status === "disabled") return "No language model is available right now.";
  if (error) return "Couldn't reach the daemon for a gist.";
  return "The model didn't answer this time.";
}
