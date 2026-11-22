import { MarkdownView } from "./MarkdownView";
import { ReadoutRows } from "./ReadoutRows";
import { stepTitle, type ToolCallItem } from "./wire";
import type { AssistantItem } from "./api";
import { ModelMark } from "@/components/ModelMark";

type ReplyItem = Extract<AssistantItem, { kind: "assistant" }>;

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
  const where =
    item.leaves_machine ??
    (provenance && model === provenance.model ? provenance.leavesMachine : undefined);
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
