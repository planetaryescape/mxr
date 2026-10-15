import { X } from "lucide-react";

import { cn } from "@/lib/utils";

import type { Hint } from "./api";

/**
 * A hint at its element: one sentence under the thing it explains, with a
 * small caret pointing up at it. It floats over the content below rather
 * than sitting in the flow: the zero-height anchor takes no space, so
 * nothing under it moves when it opens or closes. Fades in; reduced motion
 * drops the slide (base.css zeroes tw-animate's offsets).
 */
export function AnchoredHint({
  hint,
  onDismiss,
  className,
}: {
  hint: Hint;
  onDismiss: () => void;
  className?: string;
}) {
  return (
    <div className={cn("relative h-0", className)}>
      <div
        role="status"
        data-testid="hint"
        data-hint={hint.id}
        className="absolute left-0 top-1.5 z-20 flex w-max max-w-[min(60ch,100%)] items-start gap-2 rounded-md border border-primary/40 bg-primary-muted py-1.5 pl-3 pr-1.5 text-xs leading-5 text-foreground shadow-md animate-in fade-in-0 slide-in-from-top-1 duration-fast"
      >
        <span
          aria-hidden
          className="absolute -top-[5px] left-4 size-2 rotate-45 border-l border-t border-primary/40 bg-primary-muted"
        />
        <p className="min-w-0 flex-1 text-pretty">{hint.text}</p>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onDismiss();
          }}
          aria-label="Dismiss hint (Esc)"
          title="Dismiss hint (Esc)"
          className="-my-0.5 grid size-6 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <X className="size-3.5" aria-hidden />
        </button>
      </div>
    </div>
  );
}
