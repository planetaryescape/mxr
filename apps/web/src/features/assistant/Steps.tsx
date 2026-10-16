/*
 * The wire: one line per step in the daemon's words, a verb that changes
 * tense when the step ends. While a turn runs the lines show under your
 * message; when it ends they fold to one line that always opens, and every
 * step opens to what it found. The exact call is one more click, under
 * "Details" (docs/blueprint/29 "Steps are wire lines").
 */

import { useState } from "react";
import { toast } from "sonner";

import { ReadoutRows } from "./ReadoutRows";
import {
  foldSummary,
  GLYPH_WORDS,
  stepGlyph,
  stepTitle,
  type ToolCallItem,
  type WireTone,
} from "./wire";
import { undoMutation } from "@/features/mailbox/api";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { cn } from "@/lib/utils";
import { plural } from "@/lib/format";

const TONE: Record<WireTone, string> = {
  running: "text-text-2",
  done: "text-text-3",
  failed: "text-danger",
  waiting: "text-cue",
  quiet: "text-text-3",
};

export function Steps({ items, live }: { items: ToolCallItem[]; live: boolean }) {
  const settled = !live && items.every((item) => item.state !== "running");
  const [open, setOpen] = useState(false);
  if (settled && !open) {
    return <FoldedLine items={items} onOpen={() => setOpen(true)} />;
  }
  return (
    <div>
      {settled ? (
        <button
          type="button"
          onClick={() => setOpen(false)}
          aria-expanded="true"
          className="type-meta mb-1 text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1"
        >
          ▾ {plural(items.length, "step")}
        </button>
      ) : null}
      <ul className="space-y-0.5">
        {items.map((item) => (
          <Step key={item.call_id} item={item} />
        ))}
      </ul>
    </div>
  );
}

/** "3 steps · 14 found · 1 change waiting", with failures in danger. */
function FoldedLine({ items, onOpen }: { items: ToolCallItem[]; onOpen: () => void }) {
  const sum = foldSummary(items);
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-expanded="false"
      className="type-meta text-left text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1"
    >
      <span aria-hidden="true">▸ </span>
      {plural(sum.steps, "step")}
      {sum.found != null ? ` · ${sum.found} found` : null}
      {sum.changed > 0 ? ` · ${plural(sum.changed, "change")} made` : null}
      {sum.waiting > 0 ? ` · ${plural(sum.waiting, "change")} waiting` : null}
      {sum.failed > 0 ? <span className="text-danger"> · {sum.failed} failed</span> : null}
    </button>
  );
}

/** The id a mail change can be undone with, from its step's result. */
export function mutationIdOf(item: ToolCallItem): string | null {
  if (item.state !== "approved" && item.state !== "auto_approved") return null;
  const match = /"mutation_id"\s*:\s*"([^"]+)"/.exec(item.result ?? "");
  return match?.[1] ?? null;
}

function Step({ item }: { item: ToolCallItem }) {
  const [open, setOpen] = useState(false);
  const { glyph, tone } = stepGlyph(item.state);
  const mutationId = mutationIdOf(item);
  const title = stepTitle(item);
  const running = item.state === "running";
  return (
    <li>
      <div className="flex items-baseline gap-1">
        <button
          type="button"
          aria-expanded={open}
          // The model's own reason stays as the tooltip; the daemon's words are the title.
          title={item.reason || undefined}
          onClick={() => setOpen((value) => !value)}
          className={cn(
            "type-meta flex min-w-0 flex-1 items-baseline gap-2 text-left outline-none",
            "hover:text-text-1 focus-visible:text-text-1",
            TONE[tone],
          )}
        >
          <span
            role="img"
            aria-label={GLYPH_WORDS[item.state]}
            className="w-3 shrink-0 text-center"
          >
            {glyph}
          </span>
          {/* Keyed on the tense so the title crossfades when the step ends. */}
          <span key={running ? "running" : "ended"} className="wire-fade min-w-0 break-words">
            {title}
          </span>
        </button>
        {mutationId ? <UndoStep mutationId={mutationId} /> : null}
      </div>
      {open ? <StepDetails item={item} /> : null}
    </li>
  );
}

function UndoStep({ mutationId }: { mutationId: string }) {
  const [state, setState] = useState<"idle" | "undoing" | "undone">("idle");
  if (state === "undone") return <span className="type-meta text-text-3">undone</span>;
  return (
    <button
      type="button"
      disabled={state === "undoing"}
      onClick={() => {
        setState("undoing");
        undoMutation(mutationId)
          .then(() => {
            setState("undone");
            void invalidateMailQueries();
            toast.success("Undone. Tell the assistant if it should know.");
          })
          .catch((error: unknown) => {
            setState("idle");
            toast.error("Couldn't undo that", {
              description:
                error instanceof Error ? error.message : "Its undo window may have passed.",
            });
          });
      }}
      className="type-meta shrink-0 px-1 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1 disabled:opacity-50"
    >
      Undo
    </button>
  );
}

type Readout = NonNullable<ToolCallItem["readout"]>;

/** What a step found, then the exact call under "Details". */
function StepDetails({ item }: { item: ToolCallItem }) {
  return (
    <div className="mt-1 mb-2 ml-5 space-y-2">
      {item.reason ? <p className="type-meta text-text-3">{item.reason}</p> : null}
      {item.readout ? <ReadoutView readout={item.readout} /> : null}
      <Details item={item} />
    </div>
  );
}

function ReadoutView({ readout }: { readout: Readout }) {
  const rows = readout.rows ?? [];
  const facts = readout.facts ?? [];
  const total = readout.total;
  const more = total != null ? total - rows.length : 0;
  return (
    <div className="space-y-1">
      {total != null ? (
        <p className="type-body text-text-2">{total === 0 ? "Found nothing" : `Found ${total}`}</p>
      ) : null}
      <ReadoutRows rows={rows} more={more} label="What this step found" />
      {facts.length > 0 ? (
        <dl className="type-meta grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5">
          {facts.map((fact) => (
            <div key={fact.label} className="contents">
              <dt className="text-text-3">{fact.label}</dt>
              <dd className="min-w-0 truncate text-text-2">{fact.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </div>
  );
}

/** The exact call and the start of its result, for when the words aren't enough. */
function Details({ item }: { item: ToolCallItem }) {
  const [open, setOpen] = useState(false);
  if (!item.result && !item.arguments) return null;
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="type-meta text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1"
      >
        {open ? "▾" : "▸"} Details
      </button>
      {open ? (
        <div className="mt-1 space-y-1">
          <pre className="type-meta max-h-32 overflow-auto bg-ink-room p-2 break-all whitespace-pre-wrap text-text-2">
            {item.tool} {JSON.stringify(item.arguments, null, 2)}
          </pre>
          {item.result ? (
            <pre className="type-meta max-h-56 overflow-auto bg-ink-room p-2 break-all whitespace-pre-wrap text-text-2">
              {pretty(item.result)}
            </pre>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function pretty(result: string): string {
  try {
    return JSON.stringify(JSON.parse(result), null, 2);
  } catch {
    return result;
  }
}
