import { X } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";

import type { ModeGuide } from "./api";

/**
 * A mode's first-encounter card: one or two sentences and one line of keys
 * with their verbs. It shows once, at the top of the mode, the first time
 * the mode has items; Esc or the close button retires it in every client,
 * and so does using the mode's main verb. There is no next card: this is
 * not a tour.
 */
export function ModeCard({ guide, onClose }: { guide: ModeGuide; onClose: () => void }) {
  return (
    <section
      aria-label={`About ${guide.name}`}
      data-testid="mode-card"
      className="relative mx-5 mt-3 border-l-2 border-primary bg-surface px-4 py-3 pr-11"
    >
      <p className="max-w-[62ch] text-pretty text-[13px] leading-5 text-foreground/90">
        {guide.card}
      </p>
      <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground">
        {guide.card_keys.map((key) => (
          <span key={key.key} className="inline-flex items-center gap-1">
            <KeyChip>{key.key}</KeyChip> {key.verb}
          </span>
        ))}
      </p>
      <button
        type="button"
        onClick={onClose}
        aria-label="Close this note (Esc)"
        title="Close this note (Esc)"
        className="absolute right-2 top-2 grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
      >
        <X className="size-3.5" />
      </button>
    </section>
  );
}
