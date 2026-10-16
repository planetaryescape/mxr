/*
 * The assistant's reply, set as copy: the reading voice, the emails and
 * people it names as chips, the rows it refers to under it, and the mark
 * that says which model wrote it (D136). When steps before it failed and
 * weren't redone, the note under it says so, whatever the reply claims.
 */

import { MarkdownView } from "./MarkdownView";
import { ReadoutRows } from "./ReadoutRows";
import { stepTitle, type ToolCallItem } from "./wire";
import type { AssistantItem } from "./api";
import { ModelMark } from "@/components/ModelMark";

type ReplyItem = Extract<AssistantItem, { kind: "assistant" }>;

/** Which model is answering now, so a reply's mark can say where it ran. */
export interface Provenance {
  model: string | null;
  leavesMachine: boolean;
}

export function Reply({
  item,
  failed,
  provenance,
}: {
  item: ReplyItem;
  failed: ToolCallItem[];
  provenance?: Provenance;
}) {
  const model = item.model ?? provenance?.model ?? null;
  // Where it ran is only known when the model that wrote it is the one configured now.
  const where = provenance && model === provenance.model ? provenance.leavesMachine : undefined;
  return (
    <div>
      <MarkdownView
        text={item.text}
        appLinks
        className="assistant-prose type-reading text-text-1"
      />
      <ReadoutRows rows={item.rows ?? []} label="The emails and people in this reply" />
      {failed.length > 0 ? <FailedNote items={failed} /> : null}
      {model ? <ModelMark model={model} leavesMachine={where} className="mt-2 block" /> : null}
    </div>
  );
}

/**
 * Small models sometimes say they did what failed. The steps are mxr's
 * record, not the model's, so the daemon names the ones before a reply
 * that failed and weren't redone, and they show under it.
 */
function FailedNote({ items }: { items: ToolCallItem[] }) {
  return (
    <div role="note" className="mt-3 border-l border-danger pl-3">
      <p className="type-body text-danger">These didn't happen, whatever the reply says:</p>
      <ul className="type-meta mt-1 space-y-0.5 text-danger">
        {items.map((item) => (
          <li key={item.call_id} className="break-words">
            <span aria-hidden="true">✕ </span>
            {stepTitle(item)}
          </li>
        ))}
      </ul>
    </div>
  );
}
