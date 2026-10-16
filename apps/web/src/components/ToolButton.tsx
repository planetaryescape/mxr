import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { ModeFrame, type FrameWidth } from "@/components/ModeFrame";
import { READER_TOOLBAR_HEIGHT } from "@/components/PageFrame";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatChord } from "@/lib/keys/chord";
import { cn } from "@/lib/utils";

/**
 * The one quiet button style of a reader's toolbar and its end-of-reading
 * verbs (docs/blueprint/27 "Buttons and controls"): no border, secondary
 * text, the key in the tooltip rather than drawn inside the button (D130).
 * The conversation reader and the Reading reader both wear it. Menu triggers
 * that sit in the toolbar borrow the class so they match.
 */
export const TOOL_BUTTON_CLASS =
  "type-body h-7 gap-1.5 px-2 font-normal text-text-2 hover:bg-ink-lamp hover:text-text-1";

export function ToolButton({
  icon: Icon,
  label,
  keys,
  onClick,
  testId,
  hint,
  iconOnly = false,
  disabled,
}: {
  icon?: LucideIcon;
  label: string;
  keys: string;
  onClick: () => void;
  testId?: string;
  /** Extra words for the tooltip, before the key. */
  hint?: string;
  /** Just the icon, named for a screen reader and in the tooltip. */
  iconOnly?: boolean;
  disabled?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          data-testid={testId}
          aria-keyshortcuts={keys}
          aria-label={iconOnly ? `${label} (${formatChord(keys)})` : undefined}
          disabled={disabled}
          onClick={onClick}
          className={cn(TOOL_BUTTON_CLASS, iconOnly && "w-7 px-0")}
        >
          {Icon ? <Icon aria-hidden className="size-4" strokeWidth={1.75} /> : null}
          {iconOnly ? null : Icon ? <span className="max-sm:sr-only">{label}</span> : label}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{`${hint ?? label} (${formatChord(keys)})`}</TooltipContent>
    </Tooltip>
  );
}

/**
 * The row of quiet buttons above a reader's title. `wrap` lets a toolbar
 * with many labelled verbs fold onto a second line on a phone; a conversation's
 * stays at the height its list's header reserves.
 */
export function ReaderToolbar({
  wrap = false,
  width = "reader",
  children,
}: {
  wrap?: boolean;
  /** The frame of the column below it, so the toolbar starts where the title does. */
  width?: Extract<FrameWidth, "reader" | "reading">;
  children: ReactNode;
}) {
  return (
    <ModeFrame width={width}>
      <div
        className={cn("flex items-center gap-1 px-3", wrap && "flex-wrap py-1")}
        style={wrap ? { minHeight: READER_TOOLBAR_HEIGHT } : { height: READER_TOOLBAR_HEIGHT }}
      >
        {children}
      </div>
    </ModeFrame>
  );
}

/** A hairline between groups of toolbar buttons. */
export function ToolbarRule() {
  return <span className="mx-2 h-5 w-px bg-rule" aria-hidden />;
}
