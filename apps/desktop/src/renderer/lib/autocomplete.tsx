import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "./cn";

interface Suggestion {
  label: string;
  value: string;
}

export function ContactInput(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  fetchSuggestions: (query: string) => Promise<Suggestion[]>;
  inputRef?: React.RefObject<HTMLInputElement | null>;
  placeholder?: string;
}) {
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const internalRef = useRef<HTMLInputElement>(null);
  const inputRef = props.inputRef ?? internalRef;

  const fetchDebounced = useCallback(
    (query: string) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (!query || query.length < 2) {
        setSuggestions([]);
        setShowSuggestions(false);
        return;
      }
      debounceRef.current = setTimeout(async () => {
        const results = await props.fetchSuggestions(query);
        setSuggestions(results);
        setShowSuggestions(results.length > 0);
        setSelectedIndex(0);
      }, 200);
    },
    [props.fetchSuggestions],
  );

  // Extract the last segment after comma for autocomplete context
  const getActiveSegment = (value: string) => {
    const parts = value.split(",");
    return parts[parts.length - 1].trim();
  };

  const handleChange = (newValue: string) => {
    props.onChange(newValue);
    fetchDebounced(getActiveSegment(newValue));
  };

  const acceptSuggestion = (suggestion: Suggestion) => {
    const parts = props.value.split(",").map((p) => p.trim());
    parts[parts.length - 1] = suggestion.value;
    const newValue = parts.join(", ") + ", ";
    props.onChange(newValue);
    setSuggestions([]);
    setShowSuggestions(false);
    inputRef.current?.focus();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!showSuggestions) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, suggestions.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" || e.key === "Tab") {
      if (suggestions[selectedIndex]) {
        e.preventDefault();
        acceptSuggestion(suggestions[selectedIndex]);
      }
    } else if (e.key === "Escape") {
      setShowSuggestions(false);
    }
  };

  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  return (
    <label className="relative grid gap-2">
      {props.label ? <span className="mono-meta">{props.label}</span> : null}
      <input
        ref={inputRef}
        className={cn(
          "text-[length:var(--text-sm)] text-foreground outline-none placeholder:text-foreground-subtle",
          props.label
            ? "rounded border border-outline bg-panel-elevated px-4 py-3"
            : "min-w-0 flex-1 bg-transparent",
        )}
        value={props.value}
        placeholder={props.placeholder}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={() => setTimeout(() => setShowSuggestions(false), 150)}
      />
      {showSuggestions ? (
        <div
          className="absolute left-0 right-0 top-full z-10 mt-1 max-h-40 overflow-y-auto border border-outline bg-panel-elevated shadow-lg"
          style={{ borderRadius: "var(--radius-sm)" }}
        >
          {suggestions.map((s, i) => (
            <button
              key={s.value}
              type="button"
              className={cn(
                "flex w-full items-center gap-2 px-3 py-1.5 text-left text-[length:var(--text-sm)]",
                i === selectedIndex
                  ? "bg-accent/12 text-foreground"
                  : "text-foreground-muted hover:bg-panel-elevated",
              )}
              onMouseDown={(e) => {
                e.preventDefault();
                acceptSuggestion(s);
              }}
              onMouseEnter={() => setSelectedIndex(i)}
            >
              <span className="truncate">{s.label}</span>
              {s.label !== s.value ? (
                <span className="truncate text-[length:var(--text-xs)] text-foreground-subtle">
                  {s.value}
                </span>
              ) : null}
            </button>
          ))}
        </div>
      ) : null}
    </label>
  );
}
