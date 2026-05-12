import {
  Archive,
  Check,
  MailOpen,
  MessagesSquare,
  Paperclip,
  ClipboardList,
  ShieldAlert,
  Star,
  Trash2,
} from "lucide-react";
import type { MouseEvent } from "react";

import type { MessageRowView } from "./types";
import { useOptimisticMailMutation } from "./useOptimisticMailMutation";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface MailboxRowProps {
  row: MessageRowView;
  selected: boolean;
  focused: boolean;
  onOpen: () => void;
  onFocusPane: () => void;
  onToggleSelection: (shift: boolean) => void;
}

export function MailboxRow({
  row,
  selected,
  focused,
  onOpen,
  onFocusPane,
  onToggleSelection,
}: MailboxRowProps) {
  const star = useOptimisticMailMutation(row.starred ? "unstar" : "star");
  const read = useOptimisticMailMutation(row.unread ? "read" : "unread");
  const conversationCount =
    typeof row.message_count === "number" && row.message_count > 1 ? row.message_count : null;
  const openCommitmentCount =
    typeof row.open_commitment_count === "number" && row.open_commitment_count > 0
      ? row.open_commitment_count
      : null;

  function toggleSelection(event: MouseEvent) {
    event.stopPropagation();
    onToggleSelection(event.shiftKey);
  }

  return (
    <div
      role="article"
      tabIndex={0}
      aria-label={`${row.sender} ${row.subject || "(no subject)"} ${conversationCount ? `conversation thread with ${conversationCount} messages` : ""} ${openCommitmentCount ? `${openCommitmentCount} open ${openCommitmentCount === 1 ? "commitment" : "commitments"}` : ""} ${row.has_attachments ? "has attachments" : ""} ${row.snippet}`}
      onClick={onOpen}
      onFocus={onFocusPane}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen();
        }
      }}
      className={cn(
        "mailbox-row group relative grid min-w-0 cursor-pointer grid-cols-[28px_28px_minmax(148px,220px)_1fr_auto] items-center gap-3 overflow-hidden border-b border-border/70 px-3 transition-colors",
        "hover:bg-accent/70 hover:text-accent-foreground",
        selected && "bg-accent text-accent-foreground hover:bg-accent",
        focused && "bg-accent/85 text-accent-foreground ring-1 ring-ring/70 hover:bg-accent",
        row.unread &&
          "font-semibold text-foreground before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-unread-marker",
      )}
      style={{ height: "var(--row-height)" }}
    >
      <button
        type="button"
        onClick={toggleSelection}
        aria-label={selected ? "Deselect message" : "Select message"}
        className={cn(
          "grid size-4 place-items-center rounded border border-border text-[10px] text-primary opacity-0 transition-opacity group-hover:opacity-100",
          selected && "border-primary bg-primary text-primary-foreground opacity-100",
        )}
      >
        {selected ? <Check className="size-3" /> : null}
      </button>

      <button
        type="button"
        className={cn(
          "grid size-5 place-items-center rounded text-muted-foreground hover:bg-muted",
          row.starred && "text-star",
        )}
        onClick={(event) => {
          event.stopPropagation();
          star.mutate([row.id]);
        }}
        aria-label={row.starred ? "Unstar" : "Star"}
      >
        <Star className={cn("size-3.5", row.starred && "fill-current")} />
      </button>

      <div className="mailbox-row-sender flex min-w-0 items-center gap-1.5 text-[length:var(--mail-row-subject-size)]">
        <span className="min-w-0 truncate" title={row.sender_detail ?? row.sender}>
          {row.sender}
        </span>
        {conversationCount ? <ConversationBadge count={conversationCount} /> : null}
      </div>

      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <h2 className="mailbox-row-subject truncate text-[length:var(--mail-row-subject-size)] leading-5">
            {row.subject || "(no subject)"}
          </h2>
          {row.has_attachments ? (
            <Paperclip
              aria-label="Has attachments"
              className="size-3.5 shrink-0 text-foreground/75"
              role="img"
            >
              <title>{row.attachment_filename ?? "Has attachments"}</title>
            </Paperclip>
          ) : null}
          {openCommitmentCount ? <CommitmentBadge count={openCommitmentCount} /> : null}
        </div>
        <div className="mailbox-row-snippet truncate text-[length:var(--mail-row-meta-size)] font-normal leading-5 text-muted-foreground">
          {row.snippet}
        </div>
      </div>

      <div className="flex items-center gap-1 justify-self-end">
        <div className="mr-1 whitespace-nowrap font-mono text-[length:var(--mail-row-meta-size)] font-normal text-muted-foreground">
          {row.date_label}
        </div>
        <div className="hidden items-center gap-1 opacity-0 transition-opacity group-hover:flex group-hover:opacity-100">
          <QuickAction
            icon={MailOpen}
            label={row.unread ? "Mark read" : "Mark unread"}
            onClick={() => read.mutate([row.id])}
          />
          <QuickArchive id={row.id} />
        </div>
      </div>
    </div>
  );
}

function CommitmentBadge({ count }: { count: number }) {
  return (
    <Badge
      variant="outline"
      aria-label={`${count} open ${count === 1 ? "commitment" : "commitments"}`}
      title={`${count} unresolved relationship ${count === 1 ? "commitment" : "commitments"}`}
      className="h-5 shrink-0 gap-1 rounded border-amber-500/45 bg-amber-500/15 px-1.5 font-mono text-[10px] text-amber-600 dark:text-amber-300"
    >
      <ClipboardList className="size-3" aria-hidden="true" />
      {count}
    </Badge>
  );
}

function ConversationBadge({ count }: { count: number }) {
  return (
    <Badge
      variant="outline"
      aria-label={`Conversation thread with ${count} messages`}
      title={`${count} messages in this conversation`}
      className="h-5 shrink-0 gap-1 rounded border-primary/45 bg-primary/15 px-1.5 font-mono text-[10px] text-primary"
    >
      <MessagesSquare className="size-3" aria-hidden="true" />
      {count}
    </Badge>
  );
}

function QuickArchive({ id }: { id: string }) {
  const archive = useOptimisticMailMutation("archive");
  const trash = useOptimisticMailMutation("trash");
  const spam = useOptimisticMailMutation("spam");
  return (
    <>
      <QuickAction icon={Archive} label="Archive" onClick={() => archive.mutate([id])} />
      <QuickAction icon={Trash2} label="Trash" onClick={() => trash.mutate([id])} />
      <QuickAction icon={ShieldAlert} label="Spam" onClick={() => spam.mutate([id])} />
    </>
  );
}

function QuickAction({
  icon: Icon,
  label,
  onClick,
}: {
  icon: typeof Archive;
  label: string;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className="size-6"
      aria-label={label}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      <Icon className="size-3" />
    </Button>
  );
}
