import { Paperclip, Star } from "lucide-react";
import { cn } from "../lib/cn";
import type { MailboxRow } from "../../shared/types";
import { mailboxRowSelectionId } from "../lib/mailboxSelection";

export function MailRow(props: {
  row: MailboxRow;
  selected: boolean;
  multiSelected: boolean;
  pending: boolean;
  removing: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  return (
    <button
      data-testid="mail-row"
      data-row-id={mailboxRowSelectionId(props.row)}
      data-thread-id={props.row.thread_id}
      className={cn(
        "group flex w-full items-start gap-2.5 border-l-2 px-2.5 py-2 text-left transition-colors",
        "h-[var(--row-height)] min-h-[var(--row-height)]",
        props.removing && "row-exit pointer-events-none",
        props.pending && "pending-pulse",
        props.selected
          ? "border-l-accent bg-panel-elevated"
          : props.multiSelected
            ? "border-l-success/60 bg-success/6"
            : "border-l-transparent hover:bg-panel-elevated/50",
      )}
      onClick={props.onSelect}
      onDoubleClick={props.onOpen}
      onContextMenu={props.onContextMenu}
    >
      {/* Unread indicator */}
      <span
        className={cn(
          "mt-[7px] size-2 shrink-0 rounded-full transition-colors",
          props.row.unread ? "bg-accent" : "bg-transparent",
        )}
      />

      {/* Content */}
      <div className="min-w-0 flex-1">
        {/* Line 1: Sender + indicators + date */}
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-1.5">
            <span
              className={cn(
                "truncate text-[length:var(--text-sm)]",
                props.row.unread
                  ? "font-semibold text-foreground"
                  : "font-medium text-foreground-muted",
              )}
            >
              {props.row.sender}
            </span>
            {props.row.starred ? (
              <Star className="size-3.5 shrink-0 fill-warning text-warning" />
            ) : null}
            {props.row.has_attachments ? (
              <Paperclip className="size-3 shrink-0 text-foreground-subtle" />
            ) : null}
          </div>
          {props.pending ? (
            <span className="shrink-0 text-[length:var(--text-xs)] text-accent">Syncing</span>
          ) : (
            <span className="shrink-0 font-mono text-[length:var(--text-xs)] tabular-nums text-foreground-subtle">
              {props.row.date_label}
            </span>
          )}
        </div>

        {/* Line 2: Subject */}
        <p
          className={cn(
            "mt-0.5 truncate text-[length:var(--text-sm)] leading-snug",
            props.row.unread
              ? "font-medium text-foreground"
              : "text-foreground",
          )}
        >
          {props.row.subject}
        </p>

        {/* Line 3: Snippet */}
        <p className="mt-0.5 truncate text-[length:var(--text-xs)] leading-snug text-foreground-subtle">
          {props.row.snippet}
        </p>
      </div>
    </button>
  );
}

export function MailRowSkeleton() {
  return (
    <div className="flex h-[var(--row-height)] min-h-[var(--row-height)] items-start gap-2.5 border-l-2 border-l-transparent px-2.5 py-2">
      <span className="mt-[7px] size-2 shrink-0 rounded-full skeleton" />
      <div className="min-w-0 flex-1 space-y-2">
        <div className="flex items-center justify-between gap-4">
          <div className="h-3 w-28 skeleton" />
          <div className="h-3 w-12 skeleton" />
        </div>
        <div className="h-3 w-3/4 skeleton" />
        <div className="h-3 w-1/2 skeleton" />
      </div>
    </div>
  );
}

export function DateGroupHeader(props: { label: string }) {
  return (
    <div className="border-b border-outline/50 px-2.5 pb-1.5 pt-3 first:pt-1.5">
      <span className="mono-meta">{props.label}</span>
    </div>
  );
}
