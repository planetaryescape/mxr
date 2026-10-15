/*
 * What waits for the user: a change to approve or reject, or a question to
 * answer. Approving runs exactly the call shown under "Exact call"; the
 * daemon takes it from its own record, not from this page.
 */

import { Check, X } from "lucide-react";
import { useId, useState, type FormEvent } from "react";

import type { AssistantDecision, AssistantPending, AssistantQuestion } from "./api";
import { describeChange, previewDraft, previewMessages } from "./describe";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Textarea } from "@/components/ui/textarea";
import { formatWhen } from "@/lib/format";
import { cn } from "@/lib/utils";

const PREVIEW_ROWS = 6;

/** An answer being written: what is chosen and what is typed. */
interface Draft {
  selected: string[];
  other: string;
}

export function PendingCards({
  pending,
  busy,
  onDecide,
}: {
  pending: AssistantPending[];
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  return (
    <div className="space-y-3">
      {pending.map((item) =>
        item.kind === "approval" ? (
          <ApprovalCard key={item.call_id} item={item} busy={busy} onDecide={onDecide} />
        ) : (
          <QuestionCard key={item.call_id} item={item} busy={busy} onDecide={onDecide} />
        ),
      )}
    </div>
  );
}

function CardFrame({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <section
      aria-label={label}
      className="rounded-md border border-primary/40 bg-background p-3 shadow-sm shadow-black/5"
    >
      <p className="mb-1.5 font-mono text-2xs uppercase tracking-wide text-primary">{label}</p>
      {children}
    </section>
  );
}

function ApprovalCard({
  item,
  busy,
  onDecide,
}: {
  item: Extract<AssistantPending, { kind: "approval" }>;
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const messages = previewMessages(item.preview);
  const draft = messages.length === 0 ? previewDraft(item.preview) : null;
  const reject = (event?: FormEvent) => {
    event?.preventDefault();
    onDecide({ call_id: item.call_id, decision: "reject", reason: reason.trim() || undefined });
  };

  return (
    <CardFrame label="Your call">
      <h3 className="text-sm font-semibold text-foreground">
        {describeChange(item.tool, item.arguments)}
      </h3>
      {item.reason ? <p className="mt-0.5 text-xs text-muted-foreground">{item.reason}</p> : null}

      {messages.length > 0 ? (
        <ul className="mt-2 border-t border-border/60">
          {messages.slice(0, PREVIEW_ROWS).map((message) => (
            <li key={message.key} className="border-b border-border/60 py-1.5">
              <div className="flex items-baseline gap-2">
                <span className="min-w-0 flex-1 truncate text-xs font-medium">{message.subject}</span>
                {message.date ? (
                  <span className="shrink-0 font-mono text-2xs text-muted-foreground">
                    {formatWhen(message.date)}
                  </span>
                ) : null}
              </div>
              <p className="truncate text-2xs text-muted-foreground">{message.from}</p>
            </li>
          ))}
          {messages.length > PREVIEW_ROWS ? (
            <li className="py-1.5 text-2xs text-muted-foreground">
              and {messages.length - PREVIEW_ROWS} more
            </li>
          ) : null}
        </ul>
      ) : draft ? (
        <div className="mt-2 space-y-1 border-t border-border/60 pt-2 text-xs">
          <p>
            <span className="text-muted-foreground">To </span>
            {draft.to || "(no recipients)"}
          </p>
          <p className="font-medium">{draft.subject || "(no subject)"}</p>
          <p className="line-clamp-6 whitespace-pre-wrap text-muted-foreground">{draft.body}</p>
        </div>
      ) : item.preview_error ? (
        <p className="mt-2 text-xs text-warning">The preview failed: {item.preview_error}</p>
      ) : null}

      <Collapsible className="mt-2">
        <CollapsibleTrigger className="font-mono text-2xs text-muted-foreground hover:text-foreground">
          Exact call
        </CollapsibleTrigger>
        <CollapsibleContent>
          <pre className="mt-1 max-h-40 overflow-auto rounded-sm bg-muted/40 p-2 font-mono text-2xs whitespace-pre-wrap break-all">
            {item.tool} {JSON.stringify(item.arguments, null, 2)}
          </pre>
          {item.preview && messages.length === 0 && !draft ? (
            <pre className="mt-1 max-h-40 overflow-auto rounded-sm bg-muted/40 p-2 font-mono text-2xs whitespace-pre-wrap break-all">
              {JSON.stringify(item.preview, null, 2)}
            </pre>
          ) : null}
        </CollapsibleContent>
      </Collapsible>

      {rejecting ? (
        <form onSubmit={reject} className="mt-3 space-y-2">
          <Textarea
            autoFocus
            aria-label="Why, or what to do instead"
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) reject(event);
              if (event.key === "Escape") setRejecting(false);
            }}
            placeholder="Why, or what to do instead (optional)"
            className="min-h-14 text-xs"
          />
          <div className="flex gap-2">
            <Button type="submit" size="sm" variant="secondary" disabled={busy}>
              Reject
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setRejecting(false)}>
              Back
            </Button>
          </div>
        </form>
      ) : (
        <div className="mt-3 flex gap-2">
          <Button
            size="sm"
            disabled={busy}
            onClick={() => onDecide({ call_id: item.call_id, decision: "approve" })}
          >
            <Check className="size-3.5" />
            Approve
          </Button>
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => setRejecting(true)}>
            <X className="size-3.5" />
            Reject
          </Button>
        </div>
      )}
    </CardFrame>
  );
}

function QuestionCard({
  item,
  busy,
  onDecide,
}: {
  item: Extract<AssistantPending, { kind: "question" }>;
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  const [answers, setAnswers] = useState<Draft[]>(() =>
    item.questions.map(() => ({ selected: [], other: "" })),
  );
  const complete = answers.every(
    (answer) => answer.selected.length > 0 || answer.other.trim() !== "",
  );
  const update = (index: number, next: Draft) =>
    setAnswers((current) => current.map((answer, i) => (i === index ? next : answer)));
  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (!complete || busy) return;
    onDecide({
      call_id: item.call_id,
      decision: "answer",
      answers: answers.map((answer) => ({
        selected: answer.selected,
        other: answer.other.trim() || undefined,
      })),
    });
  };

  return (
    <CardFrame label="A question for you">
      <form onSubmit={submit} className="space-y-4">
        {item.questions.map((question, index) => (
          <QuestionField
            key={`${item.call_id}:${question.question}`}
            question={question}
            answer={answers[index] ?? { selected: [], other: "" }}
            onChange={(next) => update(index, next)}
            onSubmit={submit}
          />
        ))}
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={!complete || busy}>
            Answer
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={busy}
            onClick={() => onDecide({ call_id: item.call_id, decision: "reject" })}
          >
            Skip
          </Button>
        </div>
      </form>
    </CardFrame>
  );
}

function QuestionField({
  question,
  answer,
  onChange,
  onSubmit,
}: {
  question: AssistantQuestion;
  answer: Draft;
  onChange: (answer: Draft) => void;
  onSubmit: () => void;
}) {
  const otherId = useId();
  const options = question.options ?? [];
  const choose = (label: string) => {
    const chosen = answer.selected.includes(label);
    const selected = question.multi_select
      ? chosen
        ? answer.selected.filter((value) => value !== label)
        : [...answer.selected, label]
      : chosen
        ? []
        : [label];
    // A single answer is either an option or typed text, not both.
    onChange({ selected, other: question.multi_select || selected.length === 0 ? answer.other : "" });
  };

  return (
    <fieldset className="space-y-1.5">
      {question.header ? (
        <span className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          {question.header}
        </span>
      ) : null}
      <legend className="text-sm font-medium text-foreground">{question.question}</legend>
      {options.length > 0 ? (
        <div
          role={question.multi_select ? "group" : "radiogroup"}
          aria-label={question.question}
          className="space-y-1"
        >
          {options.map((option) => {
            const chosen = answer.selected.includes(option.label);
            return (
              <button
                key={option.label}
                type="button"
                role={question.multi_select ? "checkbox" : "radio"}
                aria-checked={chosen}
                onClick={() => choose(option.label)}
                className={cn(
                  "block w-full rounded-sm border px-2.5 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  chosen
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border hover:border-primary/50 hover:bg-muted/40",
                )}
              >
                <span className="block text-xs font-medium">{option.label}</span>
                {option.description ? (
                  <span className="block text-2xs text-muted-foreground">{option.description}</span>
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}
      {question.allow_other ? (
        <div>
          <label htmlFor={otherId} className="sr-only">
            Your own answer
          </label>
          <Textarea
            id={otherId}
            value={answer.other}
            onChange={(event) =>
              onChange({
                selected: question.multi_select ? answer.selected : [],
                other: event.target.value,
              })
            }
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                onSubmit();
              }
            }}
            placeholder={options.length > 0 ? "Or write your own answer" : "Your answer"}
            className="min-h-10 text-xs"
          />
        </div>
      ) : null}
    </fieldset>
  );
}
