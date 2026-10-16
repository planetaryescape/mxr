import { SIZE_RANGE, type Spacing, type Typeface } from "@planetaryescape/reader";
import { Minus, Plus, Type } from "lucide-react";
import type { ReactNode } from "react";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { READER_THEMES } from "./readerAppearance";

const TYPEFACES: { id: Typeface; label: string }[] = [
  { id: "serif", label: "Serif" },
  { id: "book", label: "Book" },
  { id: "sans", label: "Sans" },
  { id: "legible", label: "Legible" },
];

const SPACINGS: { id: Spacing; label: string }[] = [
  { id: "snug", label: "Snug" },
  { id: "even", label: "Even" },
  { id: "airy", label: "Airy" },
];

/** Theme, typeface, size and line spacing for the reading column; kept in this browser. */
export function ReaderSettingsMenu() {
  const display = useUiPrefs((state) => state.readingDisplay);
  const set = useUiPrefs((state) => state.setReadingDisplay);
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          data-testid="reader-settings"
          aria-label="Reader settings"
          title="Reader settings"
          className="type-body inline-flex h-7 items-center gap-1.5 px-2 text-text-2 hover:bg-ink-lamp hover:text-text-1"
        >
          <Type aria-hidden className="size-3.5" /> Aa
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 space-y-3 p-3 text-xs">
        <Group label="Theme">
          {READER_THEMES.map((theme) => (
            <Choice
              key={theme.id}
              checked={display.theme === theme.id}
              onSelect={() => set({ theme: theme.id })}
            >
              {theme.label}
            </Choice>
          ))}
        </Group>
        <Group label="Font">
          {TYPEFACES.map((face) => (
            <Choice
              key={face.id}
              checked={display.typeface === face.id}
              onSelect={() => set({ typeface: face.id })}
            >
              {face.label}
            </Choice>
          ))}
        </Group>
        <div className="flex items-center justify-between gap-2">
          <span className="text-muted-foreground">Size</span>
          <span className="inline-flex items-center gap-2">
            <button
              type="button"
              aria-label="Smaller text"
              disabled={display.size <= SIZE_RANGE.min}
              onClick={() => set({ size: display.size - 1 })}
              className="rounded-md border border-border p-1 hover:bg-accent disabled:opacity-40"
            >
              <Minus aria-hidden className="size-3.5" />
            </button>
            <span data-testid="reader-size" className="w-9 text-center tabular-nums">
              {display.size}px
            </span>
            <button
              type="button"
              aria-label="Larger text"
              disabled={display.size >= SIZE_RANGE.max}
              onClick={() => set({ size: display.size + 1 })}
              className="rounded-md border border-border p-1 hover:bg-accent disabled:opacity-40"
            >
              <Plus aria-hidden className="size-3.5" />
            </button>
          </span>
        </div>
        <Group label="Line spacing">
          {SPACINGS.map((spacing) => (
            <Choice
              key={spacing.id}
              checked={display.spacing === spacing.id}
              onSelect={() => set({ spacing: spacing.id })}
            >
              {spacing.label}
            </Choice>
          ))}
        </Group>
      </PopoverContent>
    </Popover>
  );
}

function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="radiogroup" aria-label={label}>
      <div className="mb-1 text-muted-foreground">{label}</div>
      <div className="flex flex-wrap gap-1">{children}</div>
    </div>
  );
}

function Choice({
  checked,
  onSelect,
  children,
}: {
  checked: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      onClick={onSelect}
      className={cn(
        "rounded-md border border-border px-2 py-0.5 hover:bg-accent",
        checked && "border-primary bg-accent text-foreground",
      )}
    >
      {children}
    </button>
  );
}
