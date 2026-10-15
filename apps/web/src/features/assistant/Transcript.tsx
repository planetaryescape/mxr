/*
 * The conversation: what the user wrote, what the assistant answered, and
 * the steps it took between, folded into one line once the turn is done.
 * The steps are the assistant's work shown, not narrated: each carries the
 * reason the model gave and, opened, the exact call and what mxr returned.
 */

import {
  ChevronRight,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  CircleSlash,
  CircleX,
  Hand,
  LoaderCircle,
} from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import type { AssistantCallState, AssistantItem } from "./api";
import { toolWords } from "./describe";
import { renderMarkdown } from "./markdown";
import { undoMutation } from "@/features/mailbox/api";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

type ToolCallItem = Extract<AssistantItem, { kind: "tool_call" }>;
type Block =
  | { kind: "user"; item: Extract<AssistantItem, { kind: "user" }> }
  | { kind: "assistant"; item: Extract<AssistantItem, { kind: "assistant" }> }
  | { kind: "summary"; item: Extract<AssistantItem, { kind: "summary" }> }
  | { kind: "steps"; items: ToolCallItem[] };

function blocks(items: AssistantItem[]): Block[] {
  const out: Block[] = [];
  for (const item of items) {
    if (item.kind === "tool_call") {
      const last = out.at(-1);
      if (last?.kind === "steps") last.items.push(item);
      else out.push({ kind: "steps", items: [item] });
    } else if (item.kind === "user") {
      out.push({ kind: "user", item });
    } else if (item.kind === "summary") {
      out.push({ kind: "summary", item });
    } else {
      out.push({ kind: "assistant", item });
    }
  }
  return out;
}

export function Transcript({
  items,
  running,
  writing = null,
}: {
  items: AssistantItem[];
  running: boolean;
  /** The reply being written now, shown until it is stored. */
  writing?: string | null;
}) {
  const grouped = useMemo(() => blocks(items), [items]);
  return (
    <ol className="space-y-4" aria-label="Conversation">
      {grouped.map((block, index) => {
        const isLast = index === grouped.length - 1;
        if (block.kind === "user") {
          return (
            <li key={`u${block.item.seq}`} className="flex justify-end">
              <p className="max-w-[88%] rounded-lg rounded-br-sm bg-muted px-3 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap text-foreground">
                {block.item.text}
              </p>
            </li>
          );
        }
        if (block.kind === "summary") {
          return (
            <li key={`sum${block.item.seq}`}>
              <Collapsible>
                <CollapsibleTrigger className="group flex w-full items-center gap-2 font-mono text-2xs tracking-wider text-muted-foreground uppercase hover:text-foreground">
                  <span className="h-px flex-1 bg-border" aria-hidden="true" />
                  Earlier messages summarised
                  <span className="h-px flex-1 bg-border" aria-hidden="true" />
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <p className="mt-2 rounded-md bg-muted/50 px-3 py-2 text-xs leading-relaxed whitespace-pre-wrap text-muted-foreground">
                    {block.item.text}
                  </p>
                </CollapsibleContent>
              </Collapsible>
            </li>
          );
        }
        if (block.kind === "assistant") {
          return (
            <li key={`a${block.item.seq}`}>
              <div
                className="assistant-prose text-sm leading-relaxed text-foreground"
                // Sanitized in renderMarkdown: no scripts, no images, safe links.
                dangerouslySetInnerHTML={{
                  __html: renderMarkdown(block.item.text),
                }}
              />
            </li>
          );
        }
        return (
          <li key={`s${block.items[0]?.call_id ?? index}`}>
            <Steps items={block.items} live={running && isLast} />
          </li>
        );
      })}
      {running && writing ? (
        <li aria-live="off">
          <div
            className="assistant-prose text-sm leading-relaxed text-foreground"
            // Sanitized in renderMarkdown: no scripts, no images, safe links.
            dangerouslySetInnerHTML={{ __html: renderMarkdown(writing) }}
          />
        </li>
      ) : null}
    </ol>
  );
}

function Steps({ items, live }: { items: ToolCallItem[]; live: boolean }) {
  const settled = !live && items.every((item) => item.state !== "running");
  const [open, setOpen] = useState(false);
  const count = items.length === 1 ? "1 step" : `${items.length} steps`;
  if (!settled || open) {
    return (
      <div>
        {settled ? (
          <button
            type="button"
            onClick={() => setOpen(false)}
            aria-expanded="true"
            className="mb-1 flex items-center gap-1.5 font-mono text-2xs text-muted-foreground hover:text-foreground"
          >
            <ChevronRight className="size-3 rotate-90" aria-hidden="true" />
            {count}
          </button>
        ) : null}
        <ul className="ml-1.5 space-y-0.5 border-l border-border pl-2">
          {items.map((item) => (
            <Step key={item.call_id} item={item} />
          ))}
        </ul>
      </div>
    );
  }
  const waiting = items.some((item) => item.state === "waiting");
  // A model can say it did something when the call failed; the count of
  // failed steps sits right above its reply so the user sees both.
  const failed = items.filter((item) => item.state === "failed").length;
  const changed = items.filter(
    (item) => item.state === "approved" || item.state === "auto_approved",
  ).length;
  return (
    <button
      type="button"
      onClick={() => setOpen(true)}
      aria-expanded="false"
      className="flex items-center gap-1.5 rounded-sm font-mono text-2xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
    >
      <ChevronRight className="size-3" aria-hidden="true" />
      {count}
      {changed > 0 ? <span className="text-primary">· {changed} changed</span> : null}
      {failed > 0 ? <span className="text-destructive">· {failed} failed</span> : null}
      {waiting ? <span className="text-warning">· waiting for you</span> : null}
    </button>
  );
}

/** The id a mail change can be undone with, from its step's result. */
export function mutationIdOf(item: ToolCallItem): string | null {
  if (item.state !== "approved" && item.state !== "auto_approved") return null;
  const match = /"mutation_id"\s*:\s*"([^"]+)"/.exec(item.result ?? "");
  return match?.[1] ?? null;
}

function UndoStep({ mutationId }: { mutationId: string }) {
  const [state, setState] = useState<"idle" | "undoing" | "undone">("idle");
  if (state === "undone") {
    return <span className="font-mono text-2xs text-muted-foreground">undone</span>;
  }
  return (
    <Button
      size="xs"
      variant="ghost"
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
    >
      Undo
    </Button>
  );
}

function Step({ item }: { item: ToolCallItem }) {
  const label = stepLabel(item);
  const mutationId = mutationIdOf(item);
  return (
    <li className="flex items-start gap-1">
      <Collapsible className="min-w-0 flex-1">
        <CollapsibleTrigger className="group flex w-full items-start gap-2 rounded-sm px-1 py-0.5 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring">
          <StateIcon state={item.state} />
          <span className="min-w-0 flex-1 text-xs text-muted-foreground group-hover:text-foreground">
            {label}
          </span>
          <ChevronRight
            className="mt-0.5 size-3 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
            aria-hidden="true"
          />
        </CollapsibleTrigger>
        <CollapsibleContent className="ml-6 space-y-1.5 py-1.5">
          <p className="font-mono text-2xs text-muted-foreground">
            {item.tool} · {STATE_WORDS[item.state]}
          </p>
          <pre className="max-h-40 overflow-auto rounded-sm bg-muted/40 p-2 font-mono text-2xs whitespace-pre-wrap break-all">
            {JSON.stringify(item.arguments, null, 2)}
          </pre>
          {item.result ? (
            <pre className="max-h-56 overflow-auto rounded-sm bg-muted/40 p-2 font-mono text-2xs whitespace-pre-wrap break-all">
              {pretty(item.result)}
            </pre>
          ) : null}
        </CollapsibleContent>
      </Collapsible>
      {mutationId ? <UndoStep mutationId={mutationId} /> : null}
    </li>
  );
}

function stepLabel(item: ToolCallItem): string {
  if (item.tool === "ask_user") {
    const answered = answeredWith(item.result);
    const reason = item.reason || "Asked you a question";
    return answered ? `${reason}: ${answered}` : reason;
  }
  return item.reason || toolWords(item.tool);
}

/** "Work, plus a note" from an ask_user result the user wrote. */
function answeredWith(result: string | null | undefined): string | null {
  if (!result) return null;
  try {
    const parsed = JSON.parse(result) as {
      answers?: Array<{ selected?: string[]; other?: string | null }>;
    };
    const parts: string[] = [];
    for (const answer of parsed.answers ?? []) {
      parts.push(...(answer.selected ?? []));
      if (answer.other) parts.push(answer.other);
    }
    return parts.length > 0 ? parts.join(", ") : null;
  } catch {
    return null;
  }
}

function pretty(result: string): string {
  try {
    return JSON.stringify(JSON.parse(result), null, 2);
  } catch {
    return result;
  }
}

const STATE_WORDS: Record<AssistantCallState, string> = {
  running: "running",
  done: "done",
  failed: "failed",
  waiting: "waiting for you",
  approved: "you approved it",
  auto_approved: "ran without asking (reversible)",
  declined: "declined",
  answered: "you answered",
  cancelled: "stopped",
  interrupted: "interrupted",
};

function StateIcon({ state }: { state: AssistantCallState }) {
  const base = "mt-0.5 size-3.5 shrink-0";
  switch (state) {
    case "running":
      return (
        <LoaderCircle className={cn(base, "animate-spin text-primary")} aria-label="Running" />
      );
    case "done":
    case "answered":
      return <CircleCheck className={cn(base, "text-muted-foreground")} aria-label="Done" />;
    case "approved":
      return <CircleCheck className={cn(base, "text-primary")} aria-label="Approved" />;
    case "auto_approved":
      return <CircleCheck className={cn(base, "text-primary")} aria-label="Ran without asking" />;
    case "waiting":
      return <Hand className={cn(base, "text-warning")} aria-label="Waiting for you" />;
    case "failed":
      return <CircleX className={cn(base, "text-destructive")} aria-label="Failed" />;
    case "declined":
    case "cancelled":
      return <CircleSlash className={cn(base, "text-muted-foreground")} aria-label="Not run" />;
    case "interrupted":
      return <CircleAlert className={cn(base, "text-warning")} aria-label="Interrupted" />;
    default:
      return <CircleDashed className={cn(base, "text-muted-foreground")} aria-hidden="true" />;
  }
}
