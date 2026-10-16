import {
  Archive,
  Bell,
  Inbox,
  ListChecks,
  MessagesSquare,
  Newspaper,
  Sun,
  type LucideIcon,
} from "lucide-react";

import { cn } from "@/lib/utils";

export type ModeGlyphId = "now" | "messages" | "todo" | "updates" | "reading" | "archive" | "inbox";

/** One glyph per mode, the same in the rail, a row's marks and toasts (docs/blueprint/27 "Iconography"). */
export const MODE_GLYPHS = {
  now: Sun,
  messages: MessagesSquare,
  todo: ListChecks,
  updates: Bell,
  reading: Newspaper,
  archive: Archive,
  inbox: Inbox,
} satisfies Record<ModeGlyphId, LucideIcon>;

const MODE_NAMES = {
  now: "Now",
  messages: "Messages",
  todo: "To do",
  updates: "Updates",
  reading: "Reading",
  archive: "Archive",
  inbox: "Inbox",
} satisfies Record<ModeGlyphId, string>;

/**
 * "This item is also in that mode": a 14px glyph in the faint text tier, with
 * the daemon's detail ("In To do: Sign the lease, act by Mon") as its tooltip
 * and accessible name. Native `title`, not a Radix tooltip, because a list
 * draws one per row.
 */
export function ModeMark({
  mode,
  detail,
  className,
}: {
  mode: ModeGlyphId;
  detail?: string;
  className?: string;
}) {
  const Glyph = MODE_GLYPHS[mode];
  const label = detail ? `${MODE_NAMES[mode]}: ${detail}` : MODE_NAMES[mode];
  return (
    <span
      role="img"
      data-testid="mode-mark"
      data-mode={mode}
      aria-label={label}
      title={detail ?? MODE_NAMES[mode]}
      className={cn("inline-flex shrink-0 text-text-3", className)}
    >
      <Glyph aria-hidden className="size-3.5" strokeWidth={1.75} />
    </span>
  );
}

/** The marks for the modes that hold an item besides the one on screen. */
export function ModeMarks({
  modes,
  className,
}: {
  modes: readonly { mode: ModeGlyphId; reason?: string; also_in?: string }[];
  className?: string;
}) {
  if (modes.length === 0) return null;
  return (
    <span className={cn("inline-flex items-center gap-1.5", className)}>
      {modes.map((entry) => (
        <ModeMark key={entry.mode} mode={entry.mode} detail={entry.also_in ?? entry.reason} />
      ))}
    </span>
  );
}

/** The `new` mark: a sender mxr hasn't placed yet. Cue colour, meta type. */
export function NewMark({ className }: { className?: string }) {
  return (
    <span data-testid="new-mark" className={cn("type-meta shrink-0 text-cue", className)}>
      new
    </span>
  );
}
