import type { LucideIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * One quiet button of the reader's toolbar and end-of-issue verbs
 * (docs/blueprint/27 "Buttons and controls"): no border, secondary text, the
 * key in the tooltip rather than drawn inside the button (D130).
 */
export function ReaderToolButton({
  icon: Icon,
  label,
  keys,
  onClick,
  testId,
  hint,
}: {
  icon?: LucideIcon;
  label: string;
  keys: string;
  onClick: () => void;
  testId?: string;
  /** Extra words for the tooltip, before the key. */
  hint?: string;
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
          onClick={onClick}
          className="type-body h-7 gap-1.5 px-2 font-normal text-text-2 hover:bg-ink-lamp hover:text-text-1"
        >
          {Icon ? <Icon aria-hidden className="size-4" strokeWidth={1.75} /> : null}
          {label}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{hint ? `${hint} (${keys})` : `${label} (${keys})`}</TooltipContent>
    </Tooltip>
  );
}
