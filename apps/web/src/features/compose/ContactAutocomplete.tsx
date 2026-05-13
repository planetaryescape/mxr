/*
 * Compose to/cc/bcc autocomplete. Fetches matches from the contacts bridge
 * route via a 200ms debounced query. ArrowDown/ArrowUp navigate, Enter or
 * click commits the selection back to the parent.
 */

import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { Input } from "@/components/ui/input";
import { fetchContactsAutocomplete } from "@/features/compose/api";

interface ContactAutocompleteProps {
  /** Aria label / accessible name. */
  label: string;
  /** Current text in the input. */
  value: string;
  /** Called when the input text changes (any keystroke). */
  onChange: (value: string) => void;
  /** Called when the user picks a contact (Enter on highlight or click). */
  onSelect: (email: string) => void;
  placeholder?: string;
}

export function ContactAutocomplete({
  label,
  value,
  onChange,
  onSelect,
  placeholder,
}: ContactAutocompleteProps) {
  const listboxId = useId();
  const [debounced, setDebounced] = useState(value);
  const [highlight, setHighlight] = useState(0);
  const [focused, setFocused] = useState(false);
  const blurTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), 200);
    return () => clearTimeout(t);
  }, [value]);

  const query = useQuery({
    queryKey: ["contacts-autocomplete", debounced],
    queryFn: () => fetchContactsAutocomplete(debounced),
    enabled: debounced.trim().length > 0,
    staleTime: 30_000,
  });

  const suggestions = useMemo(() => query.data ?? [], [query.data]);
  const open = focused && suggestions.length > 0;

  function commitAt(index: number) {
    const choice = suggestions[index];
    if (!choice) return;
    onSelect(choice.email);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (!open) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => Math.min(suggestions.length - 1, h + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => Math.max(0, h - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      commitAt(highlight);
    }
  }

  return (
    <div className="relative">
      <Input
        aria-label={label}
        aria-autocomplete="list"
        aria-controls={open ? listboxId : undefined}
        aria-expanded={open}
        role="combobox"
        value={value}
        placeholder={placeholder}
        onChange={(e) => {
          onChange(e.target.value);
          setHighlight(0);
        }}
        onFocus={() => {
          if (blurTimer.current) clearTimeout(blurTimer.current);
          setFocused(true);
        }}
        onBlur={() => {
          // Defer so click on a suggestion can register before we close the list.
          blurTimer.current = setTimeout(() => setFocused(false), 120);
        }}
        onKeyDown={onKeyDown}
      />
      {open ? (
        <ul
          id={listboxId}
          role="listbox"
          className="absolute left-0 right-0 top-full z-50 mt-1 max-h-64 overflow-auto rounded-md border border-border bg-surface shadow"
        >
          {suggestions.map((s, idx) => (
            <li
              key={s.email}
              role="option"
              aria-selected={idx === highlight}
              className={
                idx === highlight
                  ? "cursor-pointer px-3 py-1.5 text-xs bg-accent/30"
                  : "cursor-pointer px-3 py-1.5 text-xs hover:bg-accent/20"
              }
              onMouseDown={(e) => {
                e.preventDefault();
                commitAt(idx);
              }}
            >
              <div className="font-medium text-foreground">
                {s.display_name || s.email}
              </div>
              {s.display_name ? (
                <div className="font-mono text-2xs text-muted-foreground">{s.email}</div>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
