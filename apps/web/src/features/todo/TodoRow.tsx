import { useQuery } from "@tanstack/react-query";
import { memo, useEffect, useRef, useState, type ReactNode } from "react";

import { ModeMarks } from "@/components/ModeMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { Received, useReceived } from "@/components/When";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { fetchThread } from "@/features/mailbox/api";
import { otherModes, type ThreadModes } from "@/features/modes/membership";
import { formatLongDate } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";
import { cn } from "@/lib/utils";

import type { Todo } from "./api";
import { MicrosoftTodoAction } from "./MicrosoftTodoAction";
import { resolveSuggestion, useTodoHidden } from "./todoVerbs";
import {
  dueByToday,
  fieldLabel,
  linkLine,
  orderedFields,
  provenanceSummary,
  rowSecondary,
  type Band,
} from "./todoRows";

/** Under a row on `v`: why, when, field sources and evidence, a later email's offer, then the source email. */
function Details({
  todo,
  onOpenEmail,
  onOpenResolution,
}: {
  todo: Todo;
  onOpenEmail: () => void;
  onOpenResolution: (todo: Todo, threadId: string, messageId: string) => void;
}) {
  const arrived = useReceived(todo.source_date);
  const thread = useQuery({
    queryKey: ["thread", todo.thread_id ?? ""],
    queryFn: () => fetchThread(todo.thread_id ?? ""),
    enabled: Boolean(todo.thread_id),
    staleTime: 60_000,
  });
  const message =
    thread.data?.messages.find((entry) => entry.id === todo.source_message_id) ??
    thread.data?.messages.at(-1);
  const gate = linkLine(todo);
  return (
    <div
      data-testid="todo-source"
      className="type-body mx-5 mb-2 mt-1 grid gap-2 border-l border-rule pl-3 text-text-2"
    >
      <dl className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-0.5">
        {todo.source_date ? (
          <FieldLine label="Arrived">
            <time
              dateTime={todo.source_date}
              title={`Arrived ${formatLongDate(todo.source_date)}`}
              data-testid="todo-arrived"
            >
              {arrived}
            </time>
          </FieldLine>
        ) : null}
        <FieldLine label="Why">
          <span data-testid="todo-why">{todo.why}</span>
        </FieldLine>
        {todo.next ? <FieldLine label="Next">{todo.next}</FieldLine> : null}
        {orderedFields(todo.fields).map((field) => (
          <FieldLine key={field.field} label={fieldLabel(field.field)}>
            <span className="text-text-1">{field.source_label}</span>
            {field.evidence ? <span>: &ldquo;{field.evidence}&rdquo;</span> : null}
            {field.checked ? null : (
              <span data-testid="todo-field-unchecked" className="type-meta ml-2 text-text-3">
                unchecked
              </span>
            )}
          </FieldLine>
        ))}
      </dl>
      {gate ? <p>{gate}</p> : null}
      {todo.resolution ? <Resolution todo={todo} onOpenResolution={onOpenResolution} /> : null}
      <MicrosoftTodoAction key={todo.id} todo={todo} />
      <div>
        {thread.isLoading ? (
          <p>Loading the email…</p>
        ) : message ? (
          <>
            <p className="truncate">
              <span className="text-text-1">{message.sender}</span>
              {", "}
              <time dateTime={message.date} title={formatLongDate(message.date)}>
                <Received value={message.date} />
              </time>
              {" · "}
              {message.subject || "(no subject)"}
            </p>
            <p className="line-clamp-3">{message.snippet}</p>
          </>
        ) : null}
        {todo.thread_id ? (
          <Button type="button" variant="ghost" size="sm" onClick={onOpenEmail} className="mt-1">
            Open the email
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function FieldLine({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-text-3">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

/** A later email that may complete or move this row: review it, accept it or dismiss it. */
function Resolution({
  todo,
  onOpenResolution,
}: {
  todo: Todo;
  onOpenResolution: (todo: Todo, threadId: string, messageId: string) => void;
}) {
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const requestInFlight = useRef(false);
  const resolution = todo.resolution;
  const identity = `${todo.id}\u0000${resolution?.id ?? ""}\u0000${resolution?.status ?? ""}`;
  const currentIdentity = useRef(identity);
  const mounted = useRef(false);
  currentIdentity.current = identity;
  useEffect(() => {
    setPreview(null);
  }, [identity]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const run = async (action: "preview" | "accept" | "dismiss" | "undo") => {
    if (!resolution || requestInFlight.current) return;
    requestInFlight.current = true;
    setBusy(true);
    const requested = identity;
    try {
      const result = await resolveSuggestion(todo, action);
      // A late answer for an offer that is no longer the selected one must not authorise this one.
      if (
        action === "preview" &&
        result &&
        mounted.current &&
        currentIdentity.current === requested
      ) {
        setPreview(result.summary);
      }
    } finally {
      requestInFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  };

  if (resolution?.status === "pending") {
    return (
      <div data-testid="todo-resolution" className="grid gap-2">
        <p className={cn("text-text-1", preview ? "" : "line-clamp-2")}>
          Later email: &ldquo;{resolution.quote}&rdquo;
        </p>
        {preview ? <p data-testid="todo-resolution-preview">{preview}</p> : null}
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenResolution(todo, resolution.thread_id, resolution.message_id)}
          >
            Source email
          </Button>
          {!preview ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => void run("preview")}
            >
              {busy ? "Reviewing…" : "Review"}
            </Button>
          ) : (
            <>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void run("accept")}
              >
                {resolution.action === "complete" ? "Accept and complete" : "Accept new date"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => void run("dismiss")}
              >
                Dismiss offer
              </Button>
            </>
          )}
        </div>
      </div>
    );
  }
  if (resolution?.status === "accepted") {
    return (
      <p>
        Applied from &ldquo;{resolution.quote}&rdquo;.{" "}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() => void run("undo")}
        >
          Undo
        </Button>
      </p>
    );
  }
  return null;
}

export interface TodoRowProps {
  todo: Todo;
  band: Band;
  index: number;
  /** The lamp is on this row. */
  lit: boolean;
  via: LampVia;
  /** The details are open under the row (`v`). */
  details: boolean;
  /** The other modes holding this row's email, for its marks. */
  modes?: ThreadModes;
  onSelect: (todo: Todo) => void;
  onDone: (todo: Todo, via: LampVia) => void;
  onRestore: (todo: Todo) => void;
  onOpenEmail: (todo: Todo) => void;
  onToggleDetails: (todo: Todo) => void;
  onOpenResolution: (todo: Todo, threadId: string, messageId: string) => void;
  /** The hint anchored under this row, while it shows. */
  hint?: ReactNode;
}

/**
 * One thing to do, on the shared `Row`: a checkbox at Body size, the title
 * (with the amount), who it is from, and the window in words ("act by Fri",
 * in the cue colour once it is today or past). The lit row's trail becomes
 * `Space done` and `o email`. Why it is here is the status bar's; the field
 * sources and a later email's offer are one key (`v`) away.
 */
export const TodoRow = memo(function TodoRow({
  todo,
  band,
  index,
  lit,
  via,
  details,
  modes,
  onSelect,
  onDone,
  onRestore,
  onOpenEmail,
  onToggleDetails,
  onOpenResolution,
  hint,
}: TodoRowProps) {
  const leaving = useTodoHidden((s) => s.leaving.has(todo.id));
  const hide = useTodoHidden((s) => s.hide);
  // One subscription to the minute clock: the due-today flag, then the secondary line.
  const clock = useClockLabel(
    (now) => `${dueByToday(todo, now) ? "1" : "0"}${rowSecondary(todo, now) ?? ""}`,
  );
  const cue = clock.startsWith("1");
  const secondary = clock.slice(1);
  const done = band === "done";
  const { unchecked } = provenanceSummary(todo.fields);
  return (
    <Row
      data-index={index}
      data-testid="todo-row"
      data-id={todo.id}
      data-band={band}
      data-overdue={todo.overdue ? "true" : undefined}
      data-focused={lit ? "true" : undefined}
      aria-current={lit ? "true" : undefined}
      lit={lit}
      via={via}
      leaving={leaving}
      land={[todo.thread_id, todo.source_message_id]}
      onClick={() => onSelect(todo)}
      onAnimationEnd={leaving ? () => hide([todo.id]) : undefined}
      leadControl
      lead={
        <Checkbox
          checked={done}
          data-testid="todo-done"
          aria-label={done ? `Reopen ${todo.title}` : `Tick off ${todo.title}`}
          title={done ? "Reopen" : "Tick off (Space)"}
          onClick={(event) => event.stopPropagation()}
          onCheckedChange={(checked) => (checked ? onDone(todo, "pointer") : onRestore(todo))}
          className="size-4 border-text-3 bg-transparent shadow-none hover:border-text-1"
        />
      }
      primary={
        <>
          <span
            data-testid="todo-title"
            className={cn(
              band === "coming" ? "font-normal" : "font-medium",
              done && "text-text-3 line-through",
            )}
          >
            {todo.title}
          </span>
          {todo.amount ? (
            <span data-testid="todo-amount" className="type-meta ml-2 text-text-2">
              {todo.amount.display}
            </span>
          ) : null}
        </>
      }
      secondary={
        <>
          {secondary}
          {/* A phone has no room for the window in the trail, so it follows who it is from. */}
          <span className={cn("sm:hidden", cue && !done && "text-cue")}>
            {secondary ? " · " : null}
            {todo.when_label}
          </span>
        </>
      }
      trail={
        <>
          <ModeMarks modes={otherModes(modes, "todo")} />
          {unchecked > 0 ? (
            <span
              data-testid="todo-unchecked"
              title={orderedFields(todo.fields)
                .filter((field) => !field.checked)
                .map((field) => `${fieldLabel(field.field)}: ${field.source_label}`)
                .join("\n")}
            >
              unchecked
            </span>
          ) : null}
        </>
      }
      time={
        <span
          data-testid="todo-when"
          className={cn("hidden max-w-[16rem] truncate sm:inline", cue && !done && "text-cue")}
        >
          {todo.when_label}
        </span>
      }
      verbs={
        done ? (
          <>
            <RowVerb keys="Enter" label="reopen" onClick={() => onRestore(todo)} />
            <RowVerb label="details" onClick={() => onToggleDetails(todo)} />
          </>
        ) : (
          <>
            <RowVerb
              keys="e"
              label="done"
              testId="todo-tick"
              ariaLabel={`Tick off ${todo.title}`}
              onClick={() => onDone(todo, "pointer")}
            />
            {todo.thread_id ? (
              <RowVerb keys="o" label="email" onClick={() => onOpenEmail(todo)} />
            ) : null}
            <RowVerb label="details" onClick={() => onToggleDetails(todo)} />
          </>
        )
      }
      hint={
        <>
          {details ? (
            <Details
              todo={todo}
              onOpenEmail={() => onOpenEmail(todo)}
              onOpenResolution={onOpenResolution}
            />
          ) : null}
          {hint}
        </>
      }
    />
  );
});
