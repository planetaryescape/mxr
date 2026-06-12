/*
 * Recipient field for compose (To / Cc / Bcc).
 *
 * Renders the comma-joined address string as inline chips with a pending-token
 * input and a contacts autocomplete dropdown. The comma-joined string stays the
 * single source of truth (see ComposeRoute draftFingerprint): chips are a pure
 * render of `splitAddresses(value)` and every commit/removal rebuilds the string
 * via `join(", ")`. The component never calls `onChange` on mount, so loading an
 * existing draft does not mark it dirty.
 */

import { useQuery } from "@tanstack/react-query";
import emailAddresses from "email-addresses";
import { X } from "lucide-react";
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent,
  type KeyboardEvent,
  type MutableRefObject,
  type ReactNode,
  type Ref,
} from "react";

import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { fetchContactsAutocomplete } from "./api";

interface RecipientFieldProps {
  label: "To" | "Cc" | "Bcc";
  value: string;
  onChange: (value: string) => void;
  inputRef?: Ref<HTMLInputElement>;
  /** Optional controls rendered at the trailing edge of the row (e.g. Cc/Bcc toggles). */
  trailing?: ReactNode;
}

export function RecipientField({
  label,
  value,
  onChange,
  inputRef,
  trailing,
}: RecipientFieldProps) {
  const id = `compose-${label.toLowerCase()}`;
  const listboxId = useId();
  const innerRef = useRef<HTMLInputElement>(null);
  const blurTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [pending, setPending] = useState("");
  const [debounced, setDebounced] = useState("");
  const [highlight, setHighlight] = useState(0);
  const [focused, setFocused] = useState(false);
  const [closed, setClosed] = useState(false);

  const chips = useMemo(() => splitAddresses(value), [value]);

  useEffect(() => {
    const handle = setTimeout(() => setDebounced(pending), 200);
    return () => clearTimeout(handle);
  }, [pending]);

  const query = useQuery({
    queryKey: ["contacts-autocomplete", debounced],
    queryFn: () => fetchContactsAutocomplete(debounced),
    enabled: debounced.trim().length > 0,
    staleTime: 30_000,
  });

  const suggestions = useMemo(() => {
    const taken = new Set(chips.map((chip) => extractEmail(chip).toLowerCase()));
    return (query.data ?? []).filter((item) => !taken.has(item.email.toLowerCase()));
  }, [query.data, chips]);

  const open = focused && !closed && pending.trim().length > 0 && suggestions.length > 0;

  function setRefs(node: HTMLInputElement | null) {
    innerRef.current = node;
    if (typeof inputRef === "function") inputRef(node);
    else if (inputRef) (inputRef as MutableRefObject<HTMLInputElement | null>).current = node;
  }

  function addToken(token: string) {
    const trimmed = token.trim();
    if (!trimmed) return;
    // RFC 5322 parse normalises "Name <addr>" forms; unparseable input is
    // kept verbatim and rendered as an invalid chip.
    const normalized = normalizeAddress(trimmed) ?? trimmed;
    const exists = chips.some((chip) => chip.toLowerCase() === normalized.toLowerCase());
    onChange((exists ? chips : [...chips, normalized]).join(", "));
    setPending("");
    setDebounced("");
    setClosed(true);
    setHighlight(0);
  }

  function commit() {
    if (open) {
      const choice = suggestions[highlight];
      if (choice) {
        addToken(choice.email);
        return;
      }
    }
    addToken(pending);
  }

  function removeAt(index: number) {
    onChange(chips.filter((_, idx) => idx !== index).join(", "));
    innerRef.current?.focus();
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    switch (event.key) {
      case "Enter":
        event.preventDefault();
        commit();
        return;
      case ",":
      case ";":
        event.preventDefault();
        addToken(pending);
        return;
      case "Tab":
        // Commit the in-progress token but let focus advance normally.
        if (pending.trim()) commit();
        return;
      case "Backspace":
        if (pending === "" && chips.length > 0) {
          event.preventDefault();
          removeAt(chips.length - 1);
        }
        return;
      case "Escape":
        if (open) {
          event.stopPropagation();
          setClosed(true);
        }
        return;
      case "ArrowDown":
        if (open) {
          event.preventDefault();
          setHighlight((current) => Math.min(suggestions.length - 1, current + 1));
        }
        return;
      case "ArrowUp":
        if (open) {
          event.preventDefault();
          setHighlight((current) => Math.max(0, current - 1));
        }
        return;
      default:
        return;
    }
  }

  function onPaste(event: ClipboardEvent<HTMLInputElement>) {
    const text = event.clipboardData.getData("text");
    if (!text || !/[,;\n]/.test(text)) return;
    event.preventDefault();
    const tokens = parseAddressTokens(text);
    if (tokens.length === 0) return;
    const merged = [...chips];
    for (const token of tokens) {
      if (!merged.some((chip) => chip.toLowerCase() === token.toLowerCase())) merged.push(token);
    }
    onChange(merged.join(", "));
    setPending("");
    setDebounced("");
  }

  return (
    <div className="grid grid-cols-[3.25rem_minmax(0,1fr)] items-start gap-3 px-1 py-1">
      <Label htmlFor={id} className="pt-2 text-right text-xs font-medium text-muted-foreground">
        {label}
      </Label>
      <div className="flex items-start gap-2">
        <div className="relative min-w-0 flex-1">
          <div
            className={cn(
              "flex min-h-9 w-full flex-wrap items-center gap-1.5 rounded-md border border-border bg-input px-1.5 py-1",
              "transition-colors focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-ring/30",
            )}
            onMouseDown={(event) => {
              // Clicking blank space focuses the input without stealing chip clicks.
              if (event.target === event.currentTarget) innerRef.current?.focus();
            }}
          >
            {chips.map((chip, index) => {
              const invalid = !isValidAddress(chip);
              return (
              <span
                key={chip}
                title={invalid ? `Invalid address: ${chip}` : undefined}
                className={cn(
                  "inline-flex max-w-full items-center gap-1.5 rounded-full bg-muted py-0.5 pl-0.5 pr-1.5 text-xs text-foreground",
                  invalid && "bg-destructive/10 text-destructive ring-1 ring-destructive/50",
                )}
              >
                <span
                  className={cn(
                    "flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/20 text-[10px] font-semibold text-primary",
                    invalid && "bg-destructive/20 text-destructive",
                  )}
                >
                  {initials(chip)}
                </span>
                <span className="truncate" title={chip}>
                  {displayName(chip)}
                </span>
                <button
                  type="button"
                  className="shrink-0 rounded-full text-muted-foreground transition-colors hover:text-foreground"
                  onClick={() => removeAt(index)}
                  onKeyDown={(event) => {
                    if (event.key !== "Backspace" && event.key !== "Delete") return;
                    event.preventDefault();
                    removeAt(index);
                  }}
                  aria-label={`Remove ${chip}`}
                >
                  <X className="size-3" />
                </button>
              </span>
              );
            })}
            <input
              id={id}
              ref={setRefs}
              role="combobox"
              aria-autocomplete="list"
              aria-expanded={open}
              aria-controls={open ? listboxId : undefined}
              aria-activedescendant={
                open && suggestions[highlight] ? optionId(listboxId, highlight) : undefined
              }
              value={pending}
              placeholder={chips.length === 0 ? `${label}…` : ""}
              className="min-w-[8rem] flex-1 bg-transparent py-0.5 text-xs outline-none placeholder:text-muted-foreground"
              onChange={(event) => {
                setPending(event.target.value);
                setHighlight(0);
                setClosed(false);
              }}
              onKeyDown={onKeyDown}
              onPaste={onPaste}
              onFocus={() => {
                if (blurTimer.current) clearTimeout(blurTimer.current);
                setFocused(true);
              }}
              onBlur={() => {
                // Commit a dangling token so addresses are not silently dropped.
                if (pending.trim()) addToken(pending);
                blurTimer.current = setTimeout(() => setFocused(false), 120);
              }}
            />
          </div>
          {open ? (
            <ul
              id={listboxId}
              role="listbox"
              className="absolute left-0 right-0 top-full z-50 mt-1 max-h-64 overflow-auto rounded-md border border-border bg-popover py-1 shadow-lg"
            >
              {suggestions.map((suggestion, index) => (
                <li
                  key={suggestion.email}
                  id={optionId(listboxId, index)}
                  role="option"
                  aria-selected={index === highlight}
                  className={cn(
                    "flex cursor-pointer items-center gap-2 px-2.5 py-1.5",
                    index === highlight ? "bg-muted" : "hover:bg-muted/60",
                  )}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    addToken(suggestion.email);
                  }}
                  onMouseEnter={() => setHighlight(index)}
                >
                  <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-primary/20 text-[10px] font-semibold text-primary">
                    {initials(suggestion.display_name || suggestion.email)}
                  </span>
                  <span className="min-w-0">
                    <div className="truncate text-xs text-foreground">
                      {suggestion.display_name || suggestion.email}
                    </div>
                    {suggestion.display_name ? (
                      <div className="truncate font-mono text-2xs text-muted-foreground">
                        {suggestion.email}
                      </div>
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
        {trailing ? <div className="flex shrink-0 items-center gap-1 pt-1">{trailing}</div> : null}
      </div>
    </div>
  );
}

function optionId(listboxId: string, index: number): string {
  return `${listboxId}-option-${index}`;
}

function isValidAddress(value: string): boolean {
  return emailAddresses.parseOneAddress(value) !== null;
}

/** RFC 5322 parse → canonical chip text ("Name <email>" or bare email).
 * Returns null when the input isn't a parseable single mailbox. Names that
 * contain a comma fall back to the bare address — the comma-joined field
 * value is the source of truth and must stay splittable on commas. */
function normalizeAddress(value: string): string | null {
  const parsed = emailAddresses.parseOneAddress(value);
  if (!parsed || parsed.type !== "mailbox") return null;
  if (parsed.name && !parsed.name.includes(",")) return `${parsed.name} <${parsed.address}>`;
  return parsed.address;
}

/** Pasted text → chip tokens. Tries a full RFC 5322 address-list parse first
 * (handles quoted names, groups); falls back to separator splitting so partial
 * garbage still lands as editable chips. */
function parseAddressTokens(text: string): string[] {
  const parsed = emailAddresses.parseAddressList(text.replace(/[;\n]+/g, ","));
  if (parsed) {
    return parsed
      .flatMap((entry) => (entry.type === "group" ? entry.addresses : [entry]))
      .map((mailbox) =>
        mailbox.name && !mailbox.name.includes(",")
          ? `${mailbox.name} <${mailbox.address}>`
          : mailbox.address,
      );
  }
  return text
    .split(/[,;\n]+/)
    .map((token) => token.trim())
    .filter(Boolean);
}

function splitAddresses(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function extractEmail(value: string): string {
  const match = value.match(/<([^>]+)>/);
  return (match?.[1] ?? value).trim();
}

function displayName(value: string): string {
  const match = value.match(/^(.*?)\s*<[^>]+>$/);
  const name = match?.[1]?.trim();
  return name || extractEmail(value);
}

function initials(value: string): string {
  const source = displayName(value).includes("@")
    ? extractEmail(value).split("@")[0]
    : displayName(value);
  const words = (source ?? "").split(/[.\s_-]+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return (words[0]?.slice(0, 2) ?? "?").toUpperCase();
  return `${words[0]?.[0] ?? ""}${words[1]?.[0] ?? ""}`.toUpperCase();
}
