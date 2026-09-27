import {
  Archive,
  ArrowRightCircle,
  ChevronDown,
  ChevronUp,
  Clock,
  CornerDownRight,
  Download,
  FileText,
  Forward,
  Link as LinkIcon,
  Mail,
  MailX,
  Maximize2,
  Minimize2,
  MoreHorizontal,
  Paperclip,
  Reply,
  ReplyAll,
  ShieldAlert,
  Sparkles,
  Star,
  Tag,
  Trash2,
  UserRound,
  UserSearch,
  X,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { MessageLabelView, ThreadResponse } from "@/features/mailbox/types";
import { runCommand } from "@/lib/keys/controllers";
import { formatChord } from "@/lib/keys/chord";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ReaderView } from "@/state/uiPrefsStore";

import { LabelBadge } from "./LabelBadge";

const run = (command: string) => () => runCommand("reader", command);

interface ThreadHeaderProps {
  data: ThreadResponse;
  labels: MessageLabelView[];
  starred: boolean;
  canReplyAll: boolean;
  hasAttachments: boolean;
  view: ReaderView;
  full: boolean;
  position: { index: number; total: number } | null;
}

/**
 * Subject, labels and a short action bar. Every button runs the reader's key
 * command, so clicking and typing always do the same thing.
 */
export function ThreadHeader({
  data,
  labels,
  starred,
  canReplyAll,
  hasAttachments,
  view,
  full,
  position,
}: ThreadHeaderProps) {
  const people = data.thread.participants.map((person) => person.name?.trim() || person.email);
  return (
    <header className="shrink-0 border-b border-border bg-background">
      <div className="flex items-center gap-1 px-3 pt-2">
        <IconAction icon={X} label="Close" keys="Escape" command="close" />
        <IconAction
          icon={ChevronUp}
          label="Previous conversation"
          keys="N"
          command="prevThread"
          disabled={!position || position.index <= 0}
        />
        <IconAction
          icon={ChevronDown}
          label="Next conversation"
          keys="n"
          command="nextThread"
          disabled={!position || position.index >= position.total - 1}
        />
        {position ? (
          <span className="ml-1 font-mono text-2xs text-muted-foreground tabular-nums">
            {position.index + 1} of {position.total.toLocaleString()}
          </span>
        ) : null}
        <span className="mx-2 h-5 w-px bg-border" aria-hidden />
        {/*
          Persistent controls, each earning its place: Close, previous and
          next move through the queue (used on every thread); Archive is the
          most frequent verb; Snooze is deferral, a first-class verb; the
          view switch shows which view is on. Everything else lives in More,
          with its key, and in the command palette. Reply sits at the end of
          the thread (ReplyField), where reading ends.
        */}
        <IconAction icon={Archive} label="Archive" keys="e" command="archive" />
        <IconAction icon={Clock} label="Snooze" keys="Z" command="snooze" />
        <span className="ml-auto flex items-center gap-1">
          <ViewSwitch view={view} />
          <MoreMenu
            hasAttachments={hasAttachments}
            canReplyAll={canReplyAll}
            starred={starred}
            full={full}
          />
        </span>
      </div>
      <div className="px-5 pb-3 pt-2">
        <h1 className="text-balance text-lg font-semibold leading-snug tracking-tight">
          {data.thread.subject || "(no subject)"}
        </h1>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-muted-foreground">
          <span>{plural(data.thread.message_count, "message")}</span>
          {people.length > 0 ? (
            <>
              <span aria-hidden>·</span>
              <span className="truncate">
                {people.slice(0, 4).join(", ")}
                {people.length > 4 ? ` and ${people.length - 4} more` : ""}
              </span>
            </>
          ) : null}
          {labels.map((label) => (
            <LabelBadge key={label.id} label={label} />
          ))}
        </div>
      </div>
    </header>
  );
}

function IconAction({
  icon: Icon,
  label,
  keys,
  command,
  disabled,
}: {
  icon: LucideIcon;
  label: string;
  keys: string;
  command: string;
  disabled?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`${label} (${formatChord(keys)})`}
          disabled={disabled}
          onClick={run(command)}
        >
          <Icon className="size-4 text-muted-foreground" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {label} <span className="ml-1 font-mono text-muted-foreground">{formatChord(keys)}</span>
      </TooltipContent>
    </Tooltip>
  );
}

const VIEWS: { id: ReaderView; label: string; keys: string; command: string }[] = [
  { id: "formatted", label: "Formatted", keys: "H", command: "viewHtml" },
  { id: "reader", label: "Reader", keys: "R", command: "viewReader" },
  { id: "plain", label: "Plain", keys: "R or H again", command: "viewPlain" },
];

function ViewSwitch({ view }: { view: ReaderView }) {
  return (
    <div
      role="radiogroup"
      aria-label="Message view"
      className="mr-1 hidden rounded-md border border-border p-0.5 @xl:flex"
    >
      {VIEWS.map((option) => (
        <button
          key={option.id}
          type="button"
          role="radio"
          aria-checked={view === option.id}
          title={option.keys ? `${option.label} (${option.keys})` : option.label}
          // Choosing the current view is a no-op; the key commands toggle.
          onClick={() => {
            if (view !== option.id) runCommand("reader", option.command);
          }}
          className={cn(
            "rounded px-2 py-0.5 text-[12px]",
            view === option.id
              ? "bg-accent font-medium text-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

interface MoreItem {
  label: string;
  command: string;
  keys?: string;
  icon: LucideIcon;
  divider?: boolean;
}

function moreItems(state: { starred: boolean; full: boolean }): MoreItem[] {
  return [
    { label: "Reply", command: "reply", keys: "r", icon: Reply },
    { label: "Reply all", command: "replyAll", keys: "a", icon: ReplyAll },
    { label: "Forward", command: "forward", keys: "f", icon: Forward, divider: true },
    { label: state.starred ? "Unstar" : "Star", command: "toggleStar", keys: "s", icon: Star },
    { label: "Labels…", command: "label", keys: "l", icon: Tag },
    { label: "Mark unread", command: "markUnread", keys: "U", icon: Mail },
    { label: "Mark read and archive", command: "readArchive", keys: "m", icon: Archive },
    { label: "Move to label…", command: "move", keys: "v", icon: ArrowRightCircle },
    { label: "Reply later", command: "replyLater", keys: "b", icon: CornerDownRight },
    { label: "Move to Trash", command: "trash", keys: "#", icon: Trash2 },
    { label: "Mark as spam", command: "spam", keys: "!", icon: ShieldAlert },
    { label: "Unsubscribe…", command: "unsubscribe", keys: "D", icon: MailX, divider: true },
    { label: "Summarize", command: "summarize", keys: "y", icon: Sparkles },
    { label: "Thread briefing", command: "briefing", keys: "B", icon: FileText },
    { label: "Who is this sender?", command: "whois", keys: "W", icon: UserSearch },
    { label: "Sender profile", command: "senderProfile", keys: "p", icon: UserRound },
    { label: "Draft a reply with AI", command: "draftAssist", icon: Sparkles, divider: true },
    {
      label: state.full ? "Show the list" : "Full width",
      command: "fullscreen",
      keys: "F",
      icon: state.full ? Minimize2 : Maximize2,
    },
    { label: "Links", command: "links", keys: "L", icon: LinkIcon },
    { label: "Attachments", command: "attachments", keys: "A", icon: Paperclip },
    { label: "Export as Markdown", command: "exportThread", keys: "E", icon: Download },
    { label: "Raw headers", command: "headers", keys: "g h", icon: FileText },
    { label: "Open original in a new tab", command: "openOriginal", keys: "O", icon: Maximize2 },
  ];
}

function MoreMenu({
  hasAttachments,
  canReplyAll,
  starred,
  full,
}: {
  hasAttachments: boolean;
  canReplyAll: boolean;
  starred: boolean;
  full: boolean;
}) {
  const items = moreItems({ starred, full }).filter(
    (item) =>
      (item.command !== "attachments" || hasAttachments) &&
      (item.command !== "replyAll" || canReplyAll),
  );
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label="More actions">
          <MoreHorizontal className="size-4 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-[70vh] w-64 overflow-y-auto">
        {items.map((item) => (
          <div key={item.command}>
            <DropdownMenuItem onSelect={run(item.command)}>
              <item.icon className="size-3.5" />
              {item.label}
              {item.keys ? (
                <DropdownMenuShortcut>{formatChord(item.keys)}</DropdownMenuShortcut>
              ) : null}
            </DropdownMenuItem>
            {item.divider ? <DropdownMenuSeparator /> : null}
          </div>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
