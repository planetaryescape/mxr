import {
  Archive,
  Check,
  ClipboardList,
  Clock,
  Link2,
  Mail,
  MailOpen,
  Paperclip,
  Star,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { memo, type MouseEvent, type ReactNode } from "react";

import type { MessageRowView } from "./types";
import { gistText, RowGistLine } from "@/features/gists/RowGistLine";
import { useRowGist, type RowGist } from "@/features/gists/rowGists";
import { notePointerUse } from "@/lib/actions/keyHints";
import { formatWhen, initials, parseAddress, plural } from "@/lib/format";
import { cn } from "@/lib/utils";

export type RowQuickAction = "archive" | "trash" | "toggleRead" | "toggleStar" | "snooze";

interface MailboxRowProps {
  row: MessageRowView;
  domId: string;
  selected: boolean;
  focused: boolean;
  /** The conversation open in the reader. */
  open: boolean;
  /** Any row selected: every lead shows its checkbox. */
  selecting: boolean;
  readOnly?: boolean;
  onOpen: (row: MessageRowView) => void;
  onToggleSelection: (row: MessageRowView, shift: boolean) => void;
  onQuickAction: (row: MessageRowView, action: RowQuickAction) => void;
  trailingAction?: ReactNode;
}

/**
 * One conversation or message. Layout follows the list's width (a
 * container query), not the window's: a full-width list reads as one line
 * per row; the narrow list beside the reader stacks sender, subject and
 * snippet. Inner buttons are mouse affordances only; the listbox owns
 * keyboard focus and every action has a key.
 */
export const MailboxRow = memo(function MailboxRow({
  row,
  domId,
  selected,
  focused,
  open,
  selecting,
  readOnly = false,
  onOpen,
  onToggleSelection,
  onQuickAction,
  trailingAction,
}: MailboxRowProps) {
  const count =
    typeof row.message_count === "number" && row.message_count > 1 ? row.message_count : null;
  const commitments =
    typeof row.open_commitment_count === "number" && row.open_commitment_count > 0
      ? row.open_commitment_count
      : null;
  const who = displaySender(row);
  // This row's own line: when it lands, only this row re-renders.
  const gist = useRowGist(row.thread_id);
  const userLabels = (row.labels ?? []).filter((label) => label.kind === "user").slice(0, 2);

  const stop = (handler: () => void) => (event: MouseEvent) => {
    event.stopPropagation();
    handler();
  };

  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      aria-label={rowLabel(row, who, count, gist)}
      onClick={(event) => {
        if (!readOnly && (event.metaKey || event.ctrlKey)) {
          onToggleSelection(row, false);
          return;
        }
        if (!readOnly && event.shiftKey) {
          onToggleSelection(row, true);
          return;
        }
        onOpen(row);
      }}
      data-unread={row.unread ? "true" : undefined}
      data-focused={focused ? "true" : undefined}
      className={cn(
        "mail-row group relative grid cursor-default select-none items-center border-b border-border/60",
        "grid-cols-[28px_minmax(0,1fr)_auto] gap-x-3 px-3 py-2.5",
        "@2xl:grid-cols-[28px_minmax(120px,190px)_minmax(0,1fr)_auto]",
        "[[data-density=compact]_&]:py-1.5 [[data-density=comfortable]_&]:py-3.5",
        // No transition: j/k moves the highlight on every keypress, and
        // repeated keyboard actions must not animate.
        selected ? "bg-primary-muted/70" : open ? "bg-accent" : "hover:bg-accent/60",
        focused && "bg-accent",
      )}
    >
      {focused || open ? (
        <span
          aria-hidden
          className={cn(
            "absolute inset-y-0 left-0 w-[3px]",
            focused ? "bg-primary" : "bg-primary/45",
          )}
        />
      ) : null}
      {row.unread ? (
        <span
          aria-hidden
          className="absolute left-[5px] top-1/2 size-1.5 -translate-y-1/2 rounded-full bg-unread-marker"
        />
      ) : null}

      {/* Lead: avatar that becomes a checkbox. */}
      <div className="row-span-2 self-start pt-0.5 @2xl:row-span-1 @2xl:self-center @2xl:pt-0">
        {readOnly ? (
          <Avatar name={who} />
        ) : (
          // Mouse-only: rows are listbox options, which can't hold controls;
          // x selects from the keyboard.
          <span
            aria-hidden
            onClick={stop(() => onToggleSelection(row, false))}
            className="relative grid size-7 cursor-pointer place-items-center rounded-full"
          >
            <span
              className={cn(
                "transition-opacity",
                selected || selecting ? "opacity-0" : "group-hover:opacity-0",
              )}
            >
              <Avatar name={who} />
            </span>
            <span
              className={cn(
                "absolute inset-0 grid place-items-center rounded-full border transition-opacity",
                selected
                  ? "border-primary bg-primary text-primary-foreground opacity-100"
                  : cn(
                      "border-border-strong bg-background",
                      selecting ? "opacity-100" : "opacity-0 group-hover:opacity-100",
                    ),
              )}
            >
              {selected ? <Check className="size-3.5" strokeWidth={3} /> : null}
            </span>
          </span>
        )}
      </div>

      {/* Sender. Narrow: first line with the date. Wide: its own column. */}
      <div className="flex min-w-0 items-baseline gap-1.5 @2xl:col-start-2">
        <span
          className={cn(
            "min-w-0 truncate text-[length:var(--mail-row-subject-size)]",
            row.unread ? "font-semibold text-foreground" : "text-muted-foreground",
          )}
          title={row.sender_detail ?? row.sender}
        >
          {who}
        </span>
        {count ? (
          <span className="shrink-0 font-mono text-2xs text-muted-foreground" aria-hidden>
            {count}
          </span>
        ) : null}
      </div>

      {/* Subject and snippet. */}
      <div
        className={cn(
          "col-start-2 col-end-4 min-w-0 @2xl:col-start-3 @2xl:col-end-4 @2xl:row-start-1",
          "flex flex-col gap-0.5 @2xl:flex-row @2xl:items-baseline @2xl:gap-2",
        )}
      >
        <span className="flex min-w-0 items-center gap-1.5 @2xl:shrink-0 @2xl:max-w-[60%]">
          <span
            className={cn(
              "truncate text-[length:var(--mail-row-subject-size)]",
              row.unread ? "font-semibold text-foreground" : "text-foreground/90",
            )}
          >
            {row.subject || "(no subject)"}
          </span>
          {userLabels.map((label) => (
            <LabelChip key={label.id} name={label.name} />
          ))}
          {row.triage_verdict ? (
            <TriageChip
              verdict={row.triage_verdict}
              reason={row.triage_reason ?? row.triage_line}
            />
          ) : null}
        </span>
        <span className="truncate text-[length:var(--mail-row-meta-size)] text-muted-foreground [[data-density=compact]_&]:hidden @2xl:[[data-density=compact]_&]:inline">
          <span className="hidden @2xl:inline" aria-hidden>
            ·{" "}
          </span>
          {gist ? <RowGistLine gist={gist} /> : row.snippet}
        </span>
      </div>

      {/* Meta: icons and date; quick actions replace them on hover. */}
      <div className="col-start-3 row-start-1 flex items-center justify-end gap-1.5 self-start pt-px @2xl:col-start-4 @2xl:self-center @2xl:pt-0">
        {/* The hover toolbar covers only this, never a trailing action. */}
        <span className="relative flex items-center">
          <span className={cn("flex items-center gap-1.5", !readOnly && "group-hover:invisible")}>
            {commitments ? (
              <span
                title={`${plural(commitments, "open commitment")}`}
                className="flex items-center gap-0.5 font-mono text-2xs text-warning"
              >
                <ClipboardList className="size-3" aria-hidden />
                {commitments}
              </span>
            ) : null}
            {row.link_density === "heavy" ? (
              <Link2 className="size-3.5 text-muted-foreground" aria-hidden />
            ) : null}
            {row.has_attachments ? (
              <Paperclip className="size-3.5 text-muted-foreground" aria-hidden />
            ) : null}
            {row.starred ? <Star className="size-3.5 fill-star text-star" aria-hidden /> : null}
            <time
              dateTime={row.date}
              title={row.date_full}
              className={cn(
                "whitespace-nowrap font-mono text-[length:var(--mail-row-meta-size)] tabular-nums",
                row.unread ? "text-foreground" : "text-muted-foreground",
              )}
            >
              {formatWhen(row.date) || row.date_label}
            </time>
          </span>
          {readOnly ? null : (
            <span
              aria-hidden
              className="invisible absolute right-0 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-md border border-border bg-popover p-0.5 shadow-sm group-hover:visible"
            >
              <QuickButton label="Archive (e)" onClick={stop(() => onQuickAction(row, "archive"))}>
                <Archive className="size-3.5" />
              </QuickButton>
              <QuickButton label="Trash (#)" onClick={stop(() => onQuickAction(row, "trash"))}>
                <Trash2 className="size-3.5" />
              </QuickButton>
              <QuickButton
                label={row.unread ? "Mark read (I)" : "Mark unread (U)"}
                onClick={stop(() => onQuickAction(row, "toggleRead"))}
              >
                {row.unread ? <MailOpen className="size-3.5" /> : <Mail className="size-3.5" />}
              </QuickButton>
              <QuickButton label="Snooze (Z)" onClick={stop(() => onQuickAction(row, "snooze"))}>
                <Clock className="size-3.5" />
              </QuickButton>
              <QuickButton
                label={row.starred ? "Unstar (s)" : "Star (s)"}
                onClick={stop(() => onQuickAction(row, "toggleStar"))}
              >
                <Star className={cn("size-3.5", row.starred && "fill-star text-star")} />
              </QuickButton>
            </span>
          )}
        </span>
        {trailingAction ? (
          <span onClick={(event) => event.stopPropagation()} className="ml-1">
            {trailingAction}
          </span>
        ) : null}
      </div>
    </div>
  );
});

export interface RowAction {
  label: string;
  icon: LucideIcon;
  /** Accessible name for one row, e.g. "Wake Budget review now". */
  describe: (row: MessageRowView) => string;
  run: (row: MessageRowView) => void;
}

/**
 * A row's own verb, drawn as a button but not one: rows are listbox
 * options, which can't contain controls. The keyboard runs it with `w`.
 */
export function RowActionChip({ action, row }: { action: RowAction; row: MessageRowView }) {
  const Icon = action.icon;
  return (
    <span
      aria-hidden
      title={`${action.describe(row)} (w)`}
      onClick={() => {
        action.run(row);
        notePointerUse("list.row-action", action.label);
      }}
      className="flex h-6 cursor-pointer items-center gap-1 rounded-md px-2 text-[12px] text-muted-foreground hover:bg-muted hover:text-foreground"
    >
      <Icon className="size-3" /> {action.label}
    </span>
  );
}

function displaySender(row: MessageRowView): string {
  const people = (row.participants ?? [])
    .map((person) => firstName(person.name?.trim() || person.email))
    .filter(Boolean);
  if ((row.message_count ?? 1) > 1 && people.length > 1) {
    return [...new Set(people)].slice(0, 3).join(", ");
  }
  const parsed = parseAddress(row.sender);
  return parsed.name ?? row.sender ?? parsed.email ?? "Unknown sender";
}

function firstName(value: string): string {
  if (value.includes("@")) return value.split("@")[0] ?? value;
  return value.split(/\s+/)[0] ?? value;
}

function rowLabel(
  row: MessageRowView,
  who: string,
  count: number | null,
  gist: RowGist | undefined,
): string {
  return [
    row.unread ? "Unread." : null,
    row.starred ? "Starred." : null,
    who,
    row.subject || "(no subject)",
    gist ? gistText(gist) : null,
    count ? `${plural(count, "message")} in conversation` : null,
    row.has_attachments ? "Has attachments" : null,
    formatWhen(row.date),
  ]
    .filter(Boolean)
    .join(", ");
}

const AVATAR_TONES = [
  "bg-chart-1/20 text-chart-1",
  "bg-chart-2/20 text-chart-2",
  "bg-chart-3/20 text-chart-3",
  "bg-chart-4/20 text-chart-4",
  "bg-chart-5/20 text-chart-5",
  "bg-chart-6/20 text-chart-6",
];

function Avatar({ name }: { name: string }) {
  const hash = [...name].reduce((acc, char) => (acc * 31 + char.charCodeAt(0)) >>> 0, 7);
  return (
    <span
      className={cn(
        "grid size-7 place-items-center rounded-full font-mono text-[11px] font-semibold",
        "[[data-density=compact]_&]:size-6 [[data-density=compact]_&]:text-[10px]",
        AVATAR_TONES[hash % AVATAR_TONES.length],
      )}
    >
      {initials(name)}
    </span>
  );
}

function LabelChip({ name }: { name: string }) {
  return (
    <span className="hidden shrink-0 rounded-sm border border-border px-1 font-mono text-[10px] leading-4 text-muted-foreground @3xl:inline">
      {name}
    </span>
  );
}

function TriageChip({ verdict, reason }: { verdict: string; reason?: string | null }) {
  const normalized = verdict.toUpperCase();
  return (
    <span
      title={reason ?? `Triage: ${normalized}`}
      className={cn(
        "shrink-0 rounded-sm px-1 font-mono text-[10px] font-semibold leading-4",
        normalized === "ACTION" && "bg-destructive/15 text-destructive",
        normalized === "FYI" && "bg-primary-muted text-primary",
        normalized === "ROUTINE" && "bg-muted text-muted-foreground",
      )}
    >
      {normalized}
    </span>
  );
}

function QuickButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: (event: MouseEvent) => void;
  children: ReactNode;
}) {
  return (
    <span
      title={label}
      onClick={onClick}
      className="grid size-7 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-muted hover:text-foreground"
    >
      {children}
    </span>
  );
}
