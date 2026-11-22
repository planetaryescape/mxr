import { Check, ClipboardList, Link2, Paperclip, Star, type LucideIcon } from "lucide-react";
import { memo, type MouseEvent } from "react";

import type { MessageRowView } from "./types";
import { Chip } from "@/components/Chip";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { gistText, RowGistLine } from "@/features/gists/RowGistLine";
import { useRowGist, type RowGist } from "@/features/gists/rowGists";
import { notePointerUse } from "@/lib/actions/keyHints";
import { parseAddress, plural } from "@/lib/format";
import { Avatar } from "@/components/Avatar";
import { cn } from "@/lib/utils";
import { useWhen } from "@/components/When";
import { ModeChip } from "@/features/arrivals/ModeChip";
import { useRowAccountMark } from "@/features/accounts/useAccountIdentities";

export type RowQuickAction = "archive" | "trash" | "toggleRead" | "toggleStar" | "snooze";

const QUICK_VERBS: readonly { action: RowQuickAction; keys: string; label: string }[] = [
  { action: "archive", keys: "e", label: "archive" },
  { action: "snooze", keys: "b", label: "not now" },
  { action: "toggleStar", keys: "s", label: "star" },
];

interface MailboxRowProps {
  row: MessageRowView;
  domId: string;
  selected: boolean;
  position: number;
  setSize: number;
  lit: boolean;
  via: LampVia;
  selecting: boolean;
  readOnly?: boolean;
  onOpen: (row: MessageRowView) => void;
  onToggleSelection: (row: MessageRowView, shift: boolean) => void;
  onQuickAction: (row: MessageRowView, action: RowQuickAction) => void;
  rowAction?: RowAction;
}

export const MailboxRow = memo(function MailboxRow({
  row,
  domId,
  selected,
  position,
  setSize,
  lit,
  via,
  selecting,
  readOnly = false,
  onOpen,
  onToggleSelection,
  onQuickAction,
  rowAction,
}: MailboxRowProps) {
  const count =
    typeof row.message_count === "number" && row.message_count > 1 ? row.message_count : null;
  const commitments =
    typeof row.open_commitment_count === "number" && row.open_commitment_count > 0
      ? row.open_commitment_count
      : null;
  const who = displaySender(row);
  const gist = useRowGist(row.thread_id);
  const when = useWhen(row.date);
  const mark = useRowAccountMark(row.account_id);
  const userLabels = (row.labels ?? []).filter((label) => label.kind === "user").slice(0, 2);

  const stop = (handler: () => void) => (event: MouseEvent) => {
    event.stopPropagation();
    handler();
  };

  return (
    <Row
      accountId={row.account_id}
      id={domId}
      role="option"
      aria-selected={selected}
      aria-posinset={position}
      aria-setsize={setSize}
      aria-label={rowLabel(row, who, count, gist, when, mark?.name)}
      data-focused={lit ? "true" : undefined}
      lit={lit}
      via={via}
      unread={row.unread}
      className={cn("cursor-default select-none list-none", selected && "bg-primary-muted/70")}
      land={[row.thread_id, row.id]}
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
      lead={
        readOnly ? (
          <Avatar name={who} size="lg" />
        ) : (
          <span
            aria-hidden
            onClick={stop(() => onToggleSelection(row, false))}
            className="relative grid size-8 shrink-0 cursor-pointer place-items-center"
          >
            <span
              className={cn(
                via === "pointer" && "transition-opacity",
                selected || selecting ? "opacity-0" : "group-hover/row:opacity-0",
              )}
            >
              <Avatar name={who} size="lg" />
            </span>
            <span
              className={cn(
                "absolute inset-1 grid place-items-center rounded-sm border",
                via === "pointer" && "transition-opacity",
                selected
                  ? "border-primary bg-primary text-primary-foreground opacity-100"
                  : cn(
                      "border-border-strong bg-background",
                      selecting ? "opacity-100" : "opacity-0 group-hover/row:opacity-100",
                    ),
              )}
            >
              {selected ? <Check className="size-3.5" strokeWidth={3} /> : null}
            </span>
          </span>
        )
      }
      primary={
        <>
          <span title={row.sender_detail ?? row.sender}>{who}</span>
          {count ? (
            <span className="type-meta ml-1.5 text-text-3" aria-hidden>
              {count}
            </span>
          ) : null}
        </>
      }
      secondary={
        <>
          {row.triage_verdict ? (
            <span className="mr-1.5">
              <TriageChip
                verdict={row.triage_verdict}
                reason={row.triage_reason ?? row.triage_line}
              />
            </span>
          ) : null}
          <span>{row.subject || "(no subject)"}</span>
          {userLabels.map((label) => (
            <LabelChip key={label.id} name={label.name} />
          ))}
          <span className="[[data-density=compact]_&]:hidden">
            <span aria-hidden> · </span>
            {gist ? <RowGistLine gist={gist} /> : row.snippet}
          </span>
        </>
      }
      trail={
        <>
          {commitments ? (
            <span
              title={plural(commitments, "open commitment")}
              className="flex items-center gap-0.5 text-cue"
            >
              <ClipboardList className="size-3.5" aria-hidden />
              {commitments}
            </span>
          ) : null}
          {row.link_density === "heavy" ? <Link2 className="size-3.5" aria-hidden /> : null}
          {row.has_attachments ? <Paperclip className="size-3.5" aria-hidden /> : null}
          {row.starred ? <Star className="size-3.5 fill-star text-star" aria-hidden /> : null}
          {/* Where it went (Inbox only; elsewhere no chip scope is mounted). */}
          <ModeChip messageId={row.id} />
        </>
      }
      time={
        <time dateTime={row.date} title={row.date_full}>
          {when || row.date_label}
        </time>
      }
      verbs={
        rowAction || !readOnly ? (
          <>
            {rowAction ? (
              <RowVerb
                plain
                nameFrom="list"
                keys="w"
                label={rowAction.label}
                title={`${rowAction.describe(row)} (w)`}
                onClick={() => {
                  rowAction.run(row);
                  notePointerUse("list.row-action", rowAction.label);
                }}
              />
            ) : null}
            {readOnly
              ? null
              : QUICK_VERBS.map((verb) => (
                  <RowVerb
                    plain
                    nameFrom="list"
                    key={verb.action}
                    keys={verb.keys}
                    label={verb.action === "toggleStar" && row.starred ? "unstar" : verb.label}
                    onClick={() => onQuickAction(row, verb.action)}
                  />
                ))}
          </>
        ) : undefined
      }
    />
  );
});

export interface RowAction {
  label: string;
  icon: LucideIcon;
  describe: (row: MessageRowView) => string;
  run: (row: MessageRowView) => void;
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
  when: string,
  account: string | undefined,
): string {
  return [
    row.unread ? "Unread." : null,
    row.starred ? "Starred." : null,
    who,
    account ?? null,
    row.subject || "(no subject)",
    gist ? gistText(gist) : null,
    count ? `${plural(count, "message")} in conversation` : null,
    row.has_attachments ? "Has attachments" : null,
    when,
  ]
    .filter(Boolean)
    .join(", ");
}

function LabelChip({ name }: { name: string }) {
  return <Chip className="ml-1.5 hidden @3xl:inline">{name}</Chip>;
}

function TriageChip({ verdict, reason }: { verdict: string; reason?: string | null }) {
  const normalized = verdict.toUpperCase();
  if (normalized === "FYI") {
    return (
      <Chip title={reason ?? "Triage: FYI"} className="font-mono">
        {normalized}
      </Chip>
    );
  }
  return (
    <span
      title={reason ?? `Triage: ${normalized}`}
      className={cn(
        "shrink-0 rounded-sm px-1 font-mono text-2xs font-semibold leading-4",
        normalized === "ACTION" && "bg-destructive/15 text-destructive",
        normalized === "ROUTINE" && "bg-muted text-muted-foreground",
      )}
    >
      {normalized}
    </span>
  );
}
