/*
 * The conversation as a copy desk's page: your words quoted, the wire of
 * steps between (folded to one line once the turn is done), and the copy.
 * No bubbles. Emails and people the reply names open in the app, beside
 * the assistant.
 */

import { useNavigate } from "@tanstack/react-router";
import { useMemo, useState, type MouseEvent } from "react";

import type { AssistantItem } from "./api";
import { Reply, type Provenance } from "./Copy";
import { MarkdownView } from "./MarkdownView";
import { APP_LINK_ATTRIBUTE } from "./markdown";
import { Steps } from "./Steps";
import { answeredWith, type ToolCallItem } from "./wire";

export { mutationIdOf } from "./Steps";

type Block =
  | { kind: "user"; key: string; text: string }
  | { kind: "assistant"; item: Extract<AssistantItem, { kind: "assistant" }> }
  | { kind: "summary"; item: Extract<AssistantItem, { kind: "summary" }> }
  | { kind: "steps"; items: ToolCallItem[] };

function blocks(items: AssistantItem[]): Block[] {
  const out: Block[] = [];
  for (const item of items) {
    if (item.kind === "tool_call") {
      // An answered question reads as your own reply, so the page reads like a conversation.
      const answer =
        item.tool === "ask_user" && item.state === "answered" ? answeredWith(item.result) : null;
      if (answer) {
        out.push({ kind: "user", key: `q${item.call_id}`, text: answer });
        continue;
      }
      const last = out.at(-1);
      if (last?.kind === "steps") last.items.push(item);
      else out.push({ kind: "steps", items: [item] });
    } else if (item.kind === "user") {
      out.push({ kind: "user", key: `u${item.seq}`, text: item.text });
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
  provenance,
}: {
  items: AssistantItem[];
  running: boolean;
  /** The reply being written now, shown until it is stored. */
  writing?: string | null;
  provenance?: Provenance;
}) {
  const grouped = useMemo(() => blocks(items), [items]);
  const navigate = useNavigate();
  // Reply links to an email or a person open in the app, beside the
  // assistant; a modified click is left to the browser (a new tab).
  const openAppLink = (event: MouseEvent<HTMLElement>) => {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const anchor = (event.target as HTMLElement).closest<HTMLAnchorElement>(
      `a[${APP_LINK_ATTRIBUTE}]`,
    );
    const href = anchor?.getAttribute("href");
    if (!href) return;
    event.preventDefault();
    void navigate({ href });
  };
  return (
    <ol className="space-y-5" aria-label="Conversation" onClick={openAppLink}>
      {grouped.map((block, index) => {
        const isLast = index === grouped.length - 1;
        switch (block.kind) {
          case "user":
            return (
              <li key={block.key}>
                <p className="type-body border-l-2 border-text-1 pl-3 break-words whitespace-pre-wrap text-text-1">
                  {block.text}
                </p>
              </li>
            );
          case "summary":
            return (
              <li key={`sum${block.item.seq}`}>
                <Summary text={block.item.text} />
              </li>
            );
          case "assistant": {
            const failedIds = new Set(block.item.failed_call_ids ?? []);
            const failed =
              failedIds.size > 0
                ? items.filter(
                    (item): item is ToolCallItem =>
                      item.kind === "tool_call" && failedIds.has(item.call_id),
                  )
                : [];
            return (
              <li key={`a${block.item.seq}`}>
                <Reply item={block.item} failed={failed} provenance={provenance} />
              </li>
            );
          }
          case "steps":
            return (
              <li key={`s${block.items[0]?.call_id ?? index}`}>
                <Steps items={block.items} live={running && isLast} />
              </li>
            );
        }
      })}
      {running && writing ? (
        <li aria-live="off">
          <MarkdownView text={writing} className="assistant-prose type-reading text-text-1" />
        </li>
      ) : null}
    </ol>
  );
}

function Summary({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="type-eyebrow flex w-full items-center gap-2 text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1"
      >
        <span className="h-px flex-1 bg-rule-soft" aria-hidden="true" />
        Earlier messages summarised
        <span className="h-px flex-1 bg-rule-soft" aria-hidden="true" />
      </button>
      {open ? <p className="type-body mt-2 whitespace-pre-wrap text-text-2">{text}</p> : null}
    </div>
  );
}
