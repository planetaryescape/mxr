import { ModeFrame } from "@/components/ModeFrame";
import { PageEmpty } from "@/components/PageParts";
import type { TodoRunway } from "./api";

type UncertaintyProps = {
  items: TodoRunway["might_need_you"];
  onOpenSource: (threadId: string) => void;
};

export function TodoUncertainty({ items, onOpenSource }: UncertaintyProps) {
  if (!items?.length) return null;
  return (
    <details className="mx-5 mt-5" data-testid="might-need-you">
      <summary className="cursor-pointer text-text-2">
        Might need you · {items.length}
      </summary>
      <ul className="mt-2 grid gap-3">
        {items.map((item) => (
          <li key={item.asks.message_id}>
            <button
              type="button"
              className="text-left text-text-1 underline"
              onClick={() => onOpenSource(item.thread_id)}
            >
              {item.subject || "Open the source email"}
            </button>
            {item.asks.units
              .filter((unit) => unit.answer === "not_sure")
              .map((unit) => (
                <blockquote
                  key={unit.unit}
                  className="border-l border-rule pl-3 text-text-2"
                >
                  {unit.quote ??
                    "Open the source email to review this request."}
                </blockquote>
              ))}
          </li>
        ))}
      </ul>
    </details>
  );
}

export function TodoEmptyRunway({
  runway,
  body,
  onOpenSource,
}: {
  runway: TodoRunway;
  body: string;
  onOpenSource: UncertaintyProps["onOpenSource"];
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-6">
      <PageEmpty framed glyph="todo" title="Nothing here yet" body={body} />
      <ModeFrame>
        <TodoUncertainty
          items={runway.might_need_you}
          onOpenSource={onOpenSource}
        />
      </ModeFrame>
    </div>
  );
}
