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

import { PageFrame } from "@/components/PageFrame";
import { Segmented } from "@/components/Segmented";
import { ReaderToolbar, TOOL_BUTTON_CLASS, ToolbarRule, ToolButton } from "@/components/ToolButton";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { MessageLabelView, ThreadResponse } from "@/features/mailbox/types";
import { runPointerCommand } from "@/lib/actions/keyHints";
import { formatChord } from "@/lib/keys/chord";
import { plural } from "@/lib/format";
import type { ReaderView } from "@/state/uiPrefsStore";

import { LabelBadge } from "./LabelBadge";

const run = (command: string) => () => runPointerCommand("reader", command);

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
    <header className="shrink-0 bg-background">
      <ReaderToolbar>
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
          <span className="type-meta ml-1 text-text-3">
            {position.index + 1} of {plural(position.total, "conversation")}
          </span>
        ) : null}
        <ToolbarRule />
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
      </ReaderToolbar>
      <PageFrame
        width="reader"
        wrapTitle
        detail
        publishTitle={false}
        purposeTestId="thread-people"
        eyebrow={plural(data.thread.message_count, "message")}
        title={data.thread.subject || "(no subject)"}
        purpose={
          people.length > 0
            ? `${people.slice(0, 4).join(", ")}${people.length > 4 ? ` and ${people.length - 4} more` : ""}`
            : undefined
        }
        freshness={
          labels.length > 0 ? (
            <span className="flex items-center gap-1">
              {labels.map((label) => (
                <LabelBadge key={label.id} label={label} />
              ))}
            </span>
          ) : null
        }
      />
    </header>
  );
}

function IconAction({
  icon,
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
    <ToolButton
      iconOnly
      icon={icon}
      label={label}
      keys={keys}
      disabled={disabled}
      onClick={run(command)}
    />
  );
}

const VIEWS: { id: ReaderView; label: string; command: string }[] = [
  { id: "formatted", label: "Formatted", command: "viewHtml" },
  { id: "reader", label: "Reader", command: "viewReader" },
  { id: "plain", label: "Plain", command: "viewPlain" },
];

function ViewSwitch({ view }: { view: ReaderView }) {
  return (
    <Segmented
      label="Message view"
      value={view}
      options={VIEWS.map((option) => [option.id, option.label])}
      // Choosing the current view is a no-op; the key commands toggle.
      onChange={(id) => {
        const option = VIEWS.find((candidate) => candidate.id === id);
        if (option && view !== option.id) runPointerCommand("reader", option.command);
      }}
      className="mr-1 hidden @xl:inline-flex"
    />
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
        <Button
          variant="ghost"
          size="sm"
          aria-label="More actions"
          className={`${TOOL_BUTTON_CLASS} w-7 px-0`}
        >
          <MoreHorizontal aria-hidden className="size-4" strokeWidth={1.75} />
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
