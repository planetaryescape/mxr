import {
  Archive,
  Clock,
  MailCheck,
  MailOpen,
  Mail,
  ShieldAlert,
  Star,
  Tag,
  Trash2,
  X,
} from "lucide-react";
import type { ComponentType } from "react";

import { rowKey } from "./rowKey";
import type { MessageRowView } from "./types";
import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { createMailVerbs } from "@/features/mail-actions/mailVerbs";
import type { MailTarget } from "@/features/mail-actions/target";
import { plural } from "@/lib/format";
import { useSelection } from "@/state/selectionStore";

interface BulkButton {
  command: string;
  label: string;
  keys: string;
  Icon: ComponentType<{ className?: string }>;
}

const BUTTONS: BulkButton[] = [
  { command: "archive", label: "Archive", keys: "e", Icon: Archive },
  { command: "readArchive", label: "Read + archive", keys: "m", Icon: MailCheck },
  { command: "markRead", label: "Read", keys: "I", Icon: MailOpen },
  { command: "markUnread", label: "Unread", keys: "U", Icon: Mail },
  { command: "toggleStar", label: "Star", keys: "s", Icon: Star },
  { command: "label", label: "Label", keys: "l", Icon: Tag },
  { command: "snooze", label: "Snooze", keys: "Z", Icon: Clock },
  { command: "spam", label: "Spam", keys: "!", Icon: ShieldAlert },
  { command: "trash", label: "Trash", keys: "#", Icon: Trash2 },
];

/**
 * Selection bar. Buttons run the same verbs as the keys (and so the same
 * confirmation and undo), against exactly the selected rows.
 */
export function BulkActionBar({
  rows,
  getTarget,
}: {
  rows: MessageRowView[];
  getTarget: () => MailTarget | null;
}) {
  const ids = useSelection((s) => s.ids);
  const clear = useSelection((s) => s.clear);
  const selectMany = useSelection((s) => s.selectMany);
  if (ids.size === 0) return null;
  const verbs = createMailVerbs({ getTarget, composeSurface: "overlay" });
  const selected = rows.filter((row) => ids.has(rowKey(row)));
  const messageCount = selected.reduce((total, row) => total + (row.message_ids?.length ?? 1), 0);

  return (
    <div
      role="toolbar"
      aria-label="Selected conversations"
      className="absolute inset-x-3 bottom-3 z-10 flex flex-wrap items-center gap-1 rounded-lg border border-border-strong bg-popover/95 p-1.5 shadow-xl backdrop-blur"
    >
      <span className="px-2 text-[13px]">
        <span className="font-semibold">{plural(selected.length, "conversation")}</span>
        {messageCount > selected.length ? (
          <span className="ml-1 text-muted-foreground">({plural(messageCount, "message")})</span>
        ) : null}
      </span>
      {selected.length < rows.length ? (
        <Button variant="ghost" size="xs" onClick={() => selectMany(rows.map(rowKey))}>
          Select all {rows.length}
        </Button>
      ) : null}
      <span className="mx-1 h-5 w-px bg-border" aria-hidden />
      {BUTTONS.map((button) => (
        <Button
          key={button.command}
          variant="ghost"
          size="sm"
          className="h-8 gap-1.5 px-2 text-[13px]"
          title={`${button.label} (${button.keys})`}
          onClick={() => verbs[button.command]?.()}
        >
          <button.Icon className="size-3.5" />
          <span className="hidden @3xl:inline">{button.label}</span>
        </Button>
      ))}
      <Button variant="ghost" size="sm" className="ml-auto h-8 gap-1.5 px-2" onClick={clear}>
        <X className="size-3.5" />
        <KeyChip>Esc</KeyChip>
      </Button>
    </div>
  );
}
