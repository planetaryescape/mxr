import { cn } from "@/lib/utils";
import { KeyChip } from "@/components/KeyChip";
import { choiceIndex } from "@/lib/keys/choiceNavigation";

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
  keys,
}: {
  label: string;
  value: string;
  options: readonly (readonly [id: string, text: string])[];
  onChange: (value: string) => void;
  className?: string;
  /** Existing direct shortcuts, shown beside the choices they operate. */
  keys?: Readonly<Record<string, string>>;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      title="Arrow keys change the choice; Home and End jump to the first and last"
      className={cn("inline-flex border border-rule bg-transparent", className)}
    >
      {options.map(([id, text]) => (
        <button
          key={id}
          type="button"
          role="radio"
          aria-label={text}
          aria-keyshortcuts={keys?.[id]}
          aria-checked={value === id}
          tabIndex={value === id ? 0 : -1}
          onClick={() => onChange(id)}
          onKeyDown={(event) => {
            if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
            const at = options.findIndex(([option]) => option === id);
            const index = choiceIndex(event.key, at, options.length);
            if (index === null) return;
            event.preventDefault();
            const next = options[index];
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
          {keys?.[id] ? <KeyChip className="ml-1.5 h-4 px-1">{keys[id]}</KeyChip> : null}
        </button>
      ))}
    </div>
  );
}
