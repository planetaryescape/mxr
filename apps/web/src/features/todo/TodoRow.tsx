import { useQuery } from "@tanstack/react-query";
import { Check, Mail, Reply } from "lucide-react";
import { memo, type ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { useWhen } from "@/components/When";
import { fetchThread } from "@/features/mailbox/api";
import { formatLongDate } from "@/lib/format";
import { cn } from "@/lib/utils";

import type { Todo } from "./api";
import {
  fieldLabel,
  kindIcon,
  linkLine,
  orderedFields,
  primaryAction,
  provenanceSummary,
  rowParty,
  runwayFill,
  type Band,
  type PrimaryAction,
} from "./todoRows";
import { useTodoHidden } from "./todoVerbs";

/**
 * The runway bar: how much of the time between showing up and the
 * deadline has gone. One colour, the accent, never red; a row past its
 * deadline has no bar and says "was due" instead.
 */
export function RunwayBar({ fill, className }: { fill: number; className?: string }) {
  return (
    <span
      aria-hidden
      data-testid="runway-bar"
      data-fill={fill.toFixed(2)}
      className={cn(
        "relative inline-block h-1.5 w-20 shrink-0 overflow-hidden rounded-full bg-primary/15",
        className,
      )}
    >
      <span
        className="absolute inset-0 origin-left rounded-full bg-primary"
        style={{ transform: `scaleX(${fill})` }}
      />
    </span>
  );
}

const ACTION_ICONS: Record<PrimaryAction["kind"], typeof Mail> = {
  email: Mail,
  reply: Reply,
  open: Mail,
};

/** The row's one button: its verb and where it goes. */
export function ActionButton({
  action,
  onRun,
  focused,
}: {
  action: PrimaryAction;
  onRun: () => void;
  focused: boolean;
}) {
  const Icon = ACTION_ICONS[action.kind];
  return (
    <button
      type="button"
      data-testid="todo-action"
      data-kind={action.kind}
      onClick={(event) => {
        event.stopPropagation();
        onRun();
      }}
      title={
        action.kind === "email" && action.domain ? `The link goes to ${action.domain}` : undefined
      }
      className="inline-flex min-h-8 w-full min-w-0 items-center justify-center gap-1.5 rounded-md border border-border bg-background px-3 py-1 text-[12.5px] font-medium text-foreground hover:bg-accent @2xl:w-auto @2xl:justify-start"
    >
      <Icon aria-hidden className="size-3.5 shrink-0" />
      <span className="min-w-0 truncate">{action.label}</span>
      {action.kind === "email" && action.domain ? (
        <span
          data-testid="todo-action-domain"
          className="hidden min-w-0 shrink truncate font-mono text-2xs font-normal text-muted-foreground @2xl:inline"
        >
          {action.domain}
        </span>
      ) : null}
      {focused ? <KeyChip className="ml-1 hidden h-4 px-1 @2xl:inline-flex">↵</KeyChip> : null}
    </button>
  );
}

/**
 * Where each field came from: schema.org, a rule, the lead-time table, a
 * model or you. An open dot marks a field to check, such as a numeric
 * date that reads differently in UK and US order.
 */
export function ProvenanceChip({
  todo,
  expanded,
  onToggle,
}: {
  todo: Todo;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { source, unchecked } = provenanceSummary(todo.fields);
  const detail = orderedFields(todo.fields)
    .map(
      (field) =>
        `${fieldLabel(field.field)}: ${field.source_label}${field.checked ? "" : " (check this)"}`,
    )
    .join("\n");
  return (
    <button
      type="button"
      data-testid="todo-provenance"
      aria-expanded={expanded}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      title={detail}
      className="inline-flex items-center gap-1 rounded-sm border border-border px-1.5 font-mono text-2xs text-muted-foreground hover:text-foreground"
    >
      {unchecked > 0 ? (
        <span
          aria-label={`${unchecked} to check`}
          className="inline-block size-1.5 rounded-full border border-current"
        />
      ) : (
        <span aria-hidden className="inline-block size-1.5 rounded-full bg-current" />
      )}
      from {source}
      {unchecked > 0 ? `, ${unchecked} to check` : ""}
    </button>
  );
}

/** Under a row on `o`: each field with its source, then the email it came from. */
function SourcePanel({ todo, onOpenEmail }: { todo: Todo; onOpenEmail: () => void }) {
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
      className="mt-2 grid gap-2 border-l border-border pl-3 text-[12px]"
    >
      <dl className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-0.5">
        {orderedFields(todo.fields).map((field) => (
          <FieldLine key={field.field} label={fieldLabel(field.field)}>
            <span className="text-foreground/85">{field.source_label}</span>
            {field.evidence ? (
              <span className="text-muted-foreground">: &ldquo;{field.evidence}&rdquo;</span>
            ) : null}
            {field.checked ? null : (
              <span className="ml-1 text-muted-foreground">(check this)</span>
            )}
          </FieldLine>
        ))}
      </dl>
      {gate ? <p className="text-muted-foreground">{gate}</p> : null}
      <div className="text-muted-foreground">
        {thread.isLoading ? (
          <p>Loading the email…</p>
        ) : message ? (
          <>
            <p className="truncate">
              <span className="text-foreground/85">{message.sender}</span>
              {", "}
              <time dateTime={message.date} title={formatLongDate(message.date)}>
                {message.date_label}
              </time>
              {" · "}
              {message.subject || "(no subject)"}
            </p>
            <p className="line-clamp-3">{message.snippet}</p>
          </>
        ) : null}
        {todo.thread_id ? (
          <button
            type="button"
            onClick={onOpenEmail}
            className="mt-1 text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            Open the email
          </button>
        ) : null}
      </div>
    </div>
  );
}

function FieldLine({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

export interface TodoRowProps {
  todo: Todo;
  band: Band;
  index: number;
  focused: boolean;
  expanded: boolean;
  onSelect: (todo: Todo) => void;
  onPrimary: (todo: Todo) => void;
  onDone: (todo: Todo) => void;
  onRestore: (todo: Todo) => void;
  onToggleSource: (todo: Todo) => void;
  onOpenEmail: (todo: Todo) => void;
  /** The hint anchored under this row's runway bar, while it shows. */
  hint?: ReactNode;
}

/**
 * One thing to do, as an instruction: verb and object, who it is for, the
 * amount, the runway bar with "act by Wed · due Fri", and one button named
 * for what it does. Coming up rows are quieter (lighter type, no button)
 * and say when they show up; colour stays at full contrast.
 */
export const TodoRow = memo(function TodoRow({
  todo,
  band,
  index,
  focused,
  expanded,
  onSelect,
  onPrimary,
  onDone,
  onRestore,
  onToggleSource,
  onOpenEmail,
  hint,
}: TodoRowProps) {
  const leaving = useTodoHidden((s) => s.leaving.has(todo.id));
  const hide = useTodoHidden((s) => s.hide);
  const Icon = kindIcon(todo.kind);
  const party = rowParty(todo);
  const action = primaryAction(todo);
  const fill = runwayFill(todo);
  const arrived = useWhen(todo.source_date);
  const dimmed = band === "coming";
  const done = band === "done";
  return (
    <li
      data-index={index}
      data-testid="todo-row"
      data-id={todo.id}
      data-band={band}
      data-overdue={todo.overdue ? "true" : undefined}
      aria-current={focused ? "true" : undefined}
      onClick={() => onSelect(todo)}
      onAnimationEnd={leaving ? () => hide([todo.id]) : undefined}
      className={cn(
        "group/todo relative mx-2 rounded-md px-3 py-2.5",
        focused ? "bg-accent" : "hover:bg-accent/40",
        leaving && "todo-fold pointer-events-none",
      )}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-2 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <div className="flex flex-col gap-2 @2xl:flex-row @2xl:items-start @2xl:gap-4">
        <div className="flex min-w-0 flex-1 gap-3">
          <Icon aria-hidden className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
              <span
                data-testid="todo-title"
                className={cn(
                  "text-[14px] text-foreground",
                  dimmed ? "font-normal" : "font-medium",
                  done && "text-muted-foreground line-through decoration-border-strong",
                )}
              >
                {todo.title}
              </span>
              {party ? <span className="text-[13px] text-muted-foreground">{party}</span> : null}
              {todo.amount ? (
                <span
                  data-testid="todo-amount"
                  className="font-mono text-[13px] tabular-nums text-foreground/90"
                >
                  {todo.amount.display}
                </span>
              ) : null}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1">
              {fill !== null && !done ? <RunwayBar fill={fill} /> : null}
              <span
                data-testid="todo-when"
                className="font-mono text-2xs tabular-nums text-foreground/80"
              >
                {todo.when_label}
              </span>
              {todo.source_date ? (
                <time
                  data-testid="todo-arrived"
                  dateTime={todo.source_date}
                  title={`Arrived ${formatLongDate(todo.source_date)}`}
                  className="font-mono text-2xs tabular-nums text-muted-foreground"
                >
                  {arrived}
                </time>
              ) : null}
              {todo.looks_done ? (
                <span className="text-[12px] text-foreground/85">
                  Looks done: {todo.looks_done.reason}
                </span>
              ) : null}
            </div>
            {hint}
            {band !== "coming" ? (
              <p className="mt-1 text-pretty text-[12px] leading-5 text-muted-foreground">
                <span data-testid="todo-why">{todo.why}</span>
                {todo.next ? <span> {todo.next}</span> : null}{" "}
                <ProvenanceChip
                  todo={todo}
                  expanded={expanded}
                  onToggle={() => onToggleSource(todo)}
                />
              </p>
            ) : null}
            {expanded ? <SourcePanel todo={todo} onOpenEmail={() => onOpenEmail(todo)} /> : null}
          </div>
        </div>
        {done ? (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onRestore(todo);
            }}
            className="self-start text-[12px] text-muted-foreground underline decoration-border underline-offset-2 hover:text-foreground"
          >
            Reopen
          </button>
        ) : band === "coming" ? null : (
          <div className="flex w-full shrink-0 items-start gap-1 @2xl:w-auto">
            {action ? (
              <div className="grid min-w-0 flex-1 gap-0.5 @2xl:flex-none">
                <ActionButton action={action} focused={focused} onRun={() => onPrimary(todo)} />
                {action.kind === "email" && action.domain ? (
                  <span className="truncate text-center font-mono text-2xs text-muted-foreground @2xl:hidden">
                    link goes to {action.domain}
                  </span>
                ) : null}
              </div>
            ) : null}
            <button
              type="button"
              data-testid="todo-done"
              aria-label={`Tick off ${todo.title}`}
              title="Tick off (e)"
              onClick={(event) => {
                event.stopPropagation();
                onDone(todo);
              }}
              className="grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-primary-muted hover:text-primary"
            >
              <Check className="size-3.5" strokeWidth={2.25} />
            </button>
          </div>
        )}
      </div>
    </li>
  );
});
