import { useQuery } from "@tanstack/react-query";
import { Mail, Reply } from "lucide-react";
import { memo, useEffect, useRef, useState, type ReactNode } from "react";

import { Chip } from "@/components/Chip";
import { KeyChip } from "@/components/KeyChip";
import { useWhen } from "@/components/When";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { fetchThread } from "@/features/mailbox/api";
import { formatLongDate } from "@/lib/format";
import { cn } from "@/lib/utils";

import type { Todo } from "./api";
import { resolveSuggestion } from "./todoVerbs";
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
import { StatusDot } from "@/components/StatusDot";

/**
 * The runway bar: how much of the time between showing up and the
 * deadline has gone. Neutral grey, not the accent: the bar is a reading,
 * and cyan is reserved for focus, selection, the primary action and unread.
 * A row past its deadline has no bar and says "was due" instead.
 */
export function RunwayBar({ fill, className }: { fill: number; className?: string }) {
  return (
    <span
      aria-hidden
      data-testid="runway-bar"
      data-fill={fill.toFixed(2)}
      className={cn(
        "relative inline-block h-1.5 w-20 shrink-0 overflow-hidden bg-border",
        className,
      )}
    >
      <span
        className="absolute inset-0 origin-left bg-muted-foreground"
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
  // Same size and variant as the Open email button in Updates' peek.
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      data-testid="todo-action"
      data-kind={action.kind}
      onClick={(event) => {
        event.stopPropagation();
        onRun();
      }}
      title={
        action.kind === "email" && action.domain ? `The link goes to ${action.domain}` : undefined
      }
      className="w-full min-w-0 justify-center @2xl:w-auto @2xl:justify-start"
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
    </Button>
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
  className,
}: {
  todo: Todo;
  expanded: boolean;
  onToggle: () => void;
  className?: string;
}) {
  const { source, unchecked } = provenanceSummary(todo.fields);
  const detail = orderedFields(todo.fields)
    .map(
      (field) =>
        `${fieldLabel(field.field)}: ${field.source_label}${field.checked ? "" : " (check this)"}`,
    )
    .join("\n");
  return (
    <Chip asChild className={className}>
      <button
        type="button"
        data-testid="todo-provenance"
        aria-expanded={expanded}
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
        title={detail}
        className="cursor-pointer hover:text-foreground"
      >
        {unchecked > 0 ? (
          <StatusDot
            aria-hidden={false}
            aria-label={`${unchecked} to check`}
            className="size-1.5 border border-current"
          />
        ) : (
          <StatusDot className="size-1.5 bg-current" />
        )}
        from {source}
        {unchecked > 0 ? `, ${unchecked} to check` : ""}
      </button>
    </Chip>
  );
}

/** Under a row on `o`: why, when, field sources and evidence, then the source email. */
function SourcePanel({ todo, onOpenEmail }: { todo: Todo; onOpenEmail: () => void }) {
  const arrived = useWhen(todo.source_date);
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
    <div data-testid="todo-source" className="mt-2 grid gap-2 border-l border-border pl-3 text-xs">
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
  onOpenResolution: (todo: Todo, threadId: string, messageId: string) => void;
  /** The hint anchored under this row's runway bar, while it shows. */
  hint?: ReactNode;
}

/**
 * One thing to do, as an instruction: verb and object, who it is for, the
 * amount, the runway bar with "act by Wed · due Fri", and one primary action
 * named for what it does. Coming up rows are quieter (lighter type, no primary
 * action) and say when they show up; colour stays at full contrast.
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
  onOpenResolution,
  hint,
}: TodoRowProps) {
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const requestInFlight = useRef(false);
  const leaving = useTodoHidden((s) => s.leaving.has(todo.id));
  const hide = useTodoHidden((s) => s.hide);
  const Icon = kindIcon(todo.kind);
  const party = rowParty(todo);
  const action = primaryAction(todo);
  const fill = runwayFill(todo);
  const dimmed = band === "coming";
  const done = band === "done";
  const resolution = todo.resolution;
  const resolutionIdentity = `${todo.id}\u0000${resolution?.id ?? ""}\u0000${resolution?.status ?? ""}`;
  const currentResolutionIdentity = useRef(resolutionIdentity);
  const mounted = useRef(false);
  currentResolutionIdentity.current = resolutionIdentity;
  useEffect(() => {
    setPreview(null);
  }, [resolutionIdentity]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const runResolution = async (resolutionAction: "preview" | "accept" | "dismiss" | "undo") => {
    if (!resolution) return;
    if (requestInFlight.current) return;
    requestInFlight.current = true;
    setBusy(true);
    const requestIdentity = resolutionIdentity;
    try {
      const result = await resolveSuggestion(todo, resolutionAction);
      if (
        resolutionAction === "preview" &&
        result &&
        mounted.current &&
        currentResolutionIdentity.current === requestIdentity
      ) {
        setPreview(result.summary);
      }
    } finally {
      requestInFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  };
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
        <span aria-hidden className="absolute inset-y-2 left-0 w-[2px] bg-primary" />
      ) : null}
      <div className="flex flex-col gap-2 @2xl:flex-row @2xl:items-start @2xl:gap-4">
        <div className="flex min-w-0 flex-1 gap-3">
          <Checkbox
            checked={done}
            data-testid="todo-done"
            aria-label={done ? `Reopen ${todo.title}` : `Tick off ${todo.title}`}
            title={done ? "Reopen (Space or e)" : "Tick off (Space or e)"}
            onClick={(event) => event.stopPropagation()}
            onCheckedChange={(checked) => (checked ? onDone(todo) : onRestore(todo))}
            className="mt-0.5 size-8 rounded-md border-2 border-foreground/70 bg-background data-[state=checked]:border-primary"
          />
          <Icon aria-hidden className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
              <span
                data-testid="todo-title"
                className={cn(
                  "text-md text-foreground",
                  dimmed ? "font-normal" : "font-medium",
                  done && "text-muted-foreground line-through decoration-border-strong",
                )}
              >
                {todo.title}
              </span>
              {party ? <span className="text-sm text-muted-foreground">{party}</span> : null}
              {todo.amount ? (
                <span
                  data-testid="todo-amount"
                  className="font-mono text-sm tabular-nums text-foreground/90"
                >
                  {todo.amount.display}
                </span>
              ) : null}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1">
              {fill !== null && !done ? <RunwayBar fill={fill} /> : null}
              <span
                data-testid="todo-when"
                className="text-xs font-medium tabular-nums text-foreground/90"
              >
                {todo.when_label}
              </span>
              {/*
                At rest on a wide row the chip waits for focus or hover; its
                sources stay one `o` away in the expanded detail. A narrow row
                has no hover, so it always shows.
              */}
              <ProvenanceChip
                todo={todo}
                expanded={expanded}
                onToggle={() => onToggleSource(todo)}
                className={
                  focused || expanded ? undefined : "@2xl:hidden @2xl:group-hover/todo:inline-flex"
                }
              />
            </div>
            {todo.looks_done ? (
              <p className="mt-1 text-xs text-foreground/85">
                Looks done: {todo.looks_done.reason}
              </p>
            ) : null}
            {resolution?.status === "pending" ? (
              <div
                data-testid="todo-resolution"
                className="mt-2 grid gap-2 rounded-md border border-border bg-background/70 p-2 text-xs"
              >
                <p className={cn("text-foreground/90", preview ? "" : "line-clamp-2")}>
                  Later email: “{resolution.quote}”
                </p>
                {preview ? <p data-testid="todo-resolution-preview">{preview}</p> : null}
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() =>
                      onOpenResolution(todo, resolution.thread_id, resolution.message_id)
                    }
                    className="min-h-10"
                  >
                    Source email
                  </Button>
                  {!preview ? (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={busy}
                      onClick={() => void runResolution("preview")}
                      className="min-h-10"
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
                        onClick={() => void runResolution("accept")}
                        className="min-h-10"
                      >
                        {resolution.action === "complete"
                          ? "Accept and complete"
                          : "Accept new date"}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={busy}
                        onClick={() => void runResolution("dismiss")}
                        className="min-h-10"
                      >
                        Dismiss offer
                      </Button>
                    </>
                  )}
                </div>
              </div>
            ) : null}
            {resolution?.status === "accepted" ? (
              <p className="mt-2 text-xs text-muted-foreground">
                Applied from “{resolution.quote}”.{" "}
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() => void runResolution("undo")}
                  className="min-h-10"
                >
                  Undo
                </Button>
              </p>
            ) : null}
            {hint}
            {expanded ? <SourcePanel todo={todo} onOpenEmail={() => onOpenEmail(todo)} /> : null}
          </div>
        </div>
        {done || band === "coming" ? null : (
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
          </div>
        )}
      </div>
    </li>
  );
});
