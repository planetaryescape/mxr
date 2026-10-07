import { X } from "lucide-react";

import { cn } from "@/lib/utils";

import type { Hint } from "./api";

/**
 * A hint at its element: one sentence under the thing it explains, with a
 * small caret pointing up at it. It sits in the flow rather than floating,
 * so it never covers the row it describes or the one under it. Fades in;
 * reduced motion drops the slide (base.css zeroes tw-animate's offsets).
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
    <div
      role="status"
      data-testid="hint"
      data-hint={hint.id}
      className={cn(
        "relative mt-1.5 flex max-w-[60ch] items-start gap-2 rounded-md border border-primary/40 bg-primary-muted py-1.5 pl-3 pr-1.5 text-[12.5px] leading-5 text-foreground animate-in fade-in-0 slide-in-from-top-1 duration-fast",
        className,
      )}
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
  );
}
