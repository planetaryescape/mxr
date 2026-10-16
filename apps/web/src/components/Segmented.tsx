import { cn } from "@/lib/utils";

/**
 * The one either-or control (docs/blueprint/27 "Buttons and controls"): a
 * square group, the selected segment in `--signal-soft` with primary text.
 * Every two-or-more-way choice shows which option is selected. Arrow keys move
 * between segments, as a radio group does.
 */
export function Segmented({
  label,
  value,
  options,
  onChange,
  className,
}: {
  label: string;
  value: string;
  options: readonly (readonly [id: string, text: string])[];
  onChange: (value: string) => void;
  className?: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className={cn("inline-flex border border-rule bg-transparent", className)}
    >
      {options.map(([id, text]) => (
        <button
          key={id}
          type="button"
          role="radio"
          aria-checked={value === id}
          tabIndex={value === id ? 0 : -1}
          onClick={() => onChange(id)}
          onKeyDown={(event) => {
            const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
            if (step === 0) return;
            event.preventDefault();
            const at = options.findIndex(([option]) => option === id);
            const next = options[(at + step + options.length) % options.length];
            if (!next) return;
            onChange(next[0]);
            const group = event.currentTarget.parentElement;
            group?.querySelectorAll<HTMLElement>('[role="radio"]')[options.indexOf(next)]?.focus();
          }}
          className={cn(
            "type-body h-7 whitespace-nowrap px-2.5 outline-none focus-visible:ring-2 focus-visible:ring-signal",
            value === id
              ? "bg-signal-soft text-text-1"
              : "text-text-2 hover:bg-ink-lamp hover:text-text-1",
          )}
        >
          {text}
        </button>
      ))}
    </div>
  );
}
