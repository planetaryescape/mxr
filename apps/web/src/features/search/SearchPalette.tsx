import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Mail, Search } from "lucide-react";
import type { KeyboardEvent } from "react";
import { useEffect, useRef, useState } from "react";

import { fetchSearch } from "@/features/search/api";
import type { MessageRowView } from "@/features/mailbox/types";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { runReplaceableQuery } from "@/lib/requestCoordinator";
import { cn } from "@/lib/utils";
import { useModals } from "@/state/modalStore";

export function SearchPalette() {
  const navigate = useNavigate();
  const open = useModals((state) => state.searchPaletteOpen);
  const setOpen = useModals((state) => state.setSearchPaletteOpen);
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [activeIndex, setActiveIndex] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setActiveIndex(-1);
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  useEffect(() => {
    const handle = window.setTimeout(() => setDebounced(query.trim()), 120);
    return () => window.clearTimeout(handle);
  }, [query]);

  const suggestions = useQuery({
    queryKey: ["search-palette", debounced, "mailbox-index"],
    queryFn: ({ signal }) =>
      runReplaceableQuery("search-palette", signal, (combinedSignal) =>
        fetchSearch(
          { q: debounced, mode: "lexical", sort: "relevance", scope: "messages", limit: 8 },
          { signal: combinedSignal },
        ),
      ),
    enabled: open && debounced.length > 1,
    staleTime: 15_000,
  });

  const rows = suggestions.data?.groups.flatMap((group) => group.rows).slice(0, 8) ?? [];
  const activeRowId = activeIndex >= 0 ? `search-palette-result-${activeIndex}` : undefined;

  useEffect(() => {
    setActiveIndex((current) => {
      if (rows.length === 0) return -1;
      return current >= rows.length ? rows.length - 1 : current;
    });
  }, [rows.length]);

  function close() {
    setOpen(false);
    setActiveIndex(-1);
  }

  function goToSearch() {
    const trimmed = query.trim();
    if (!trimmed) return;
    close();
    void navigate({ to: "/search", search: { q: trimmed, mode: "lexical", sort: "relevance" } });
  }

  function openRow(row: MessageRowView) {
    close();
    void navigate({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "inbox", threadId: row.thread_id },
    });
  }

  function move(delta: number) {
    if (rows.length === 0) return;
    setActiveIndex((current) => {
      const next = current + delta;
      if (next < 0) return rows.length - 1;
      if (next >= rows.length) return 0;
      return next;
    });
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || (event.ctrlKey && event.key.toLowerCase() === "j")) {
      event.preventDefault();
      event.stopPropagation();
      move(1);
    } else if (event.key === "ArrowUp" || (event.ctrlKey && event.key.toLowerCase() === "k")) {
      event.preventDefault();
      event.stopPropagation();
      move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      // Cmd/Ctrl+Enter always opens the full search page, even when a
      // quick result is highlighted. Plain Enter opens the highlighted
      // result (or falls back to full search when nothing is selected).
      if (event.metaKey || event.ctrlKey) {
        goToSearch();
        return;
      }
      const row = activeIndex >= 0 ? rows[activeIndex] : undefined;
      if (row) openRow(row);
      else goToSearch();
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="top-[18vh] max-h-[74vh] translate-y-0 gap-0 overflow-hidden rounded-xl border-border/80 bg-popover/95 p-0 shadow-2xl backdrop-blur sm:max-w-[720px]"
        onKeyDownCapture={handleKeyDown}
      >
        <DialogTitle className="sr-only">Search mail</DialogTitle>
        <div className="flex items-center border-b border-border bg-muted/30 px-4">
          <Search className="mr-2 size-3.5 shrink-0 text-muted-foreground" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(-1);
            }}
            placeholder="Search local mail..."
            aria-label="Search mail"
            aria-controls="search-palette-results"
            aria-activedescendant={activeRowId}
            className="h-12 !border-0 bg-transparent px-0 text-sm !outline-none !ring-0 !ring-offset-0 shadow-none focus:outline-none focus-visible:!outline-none focus-visible:!ring-0 focus-visible:!ring-offset-0"
          />
        </div>

        <div className="max-h-[56vh] overflow-y-auto p-2">
          {query.trim().length <= 1 ? (
            <div className="px-3 py-8 text-center text-xs text-muted-foreground">
              Type at least two characters to search mail.
            </div>
          ) : suggestions.isLoading ? (
            <div className="px-3 py-8 text-center text-xs text-muted-foreground">
              Searching local mail...
            </div>
          ) : rows.length > 0 ? (
            <div id="search-palette-results" className="grid gap-1" role="listbox">
              {rows.map((row, index) => (
                <button
                  key={row.id}
                  id={`search-palette-result-${index}`}
                  type="button"
                  role="option"
                  aria-selected={activeIndex === index}
                  className={cn(
                    "grid w-full grid-cols-[24px_minmax(0,1fr)_auto] items-center gap-3 rounded-md border-l-2 px-3 py-2 text-left text-xs outline-none transition-colors",
                    activeIndex === index
                      ? "border-l-primary bg-accent text-accent-foreground ring-1 ring-ring"
                      : "border-l-transparent hover:bg-muted/70",
                  )}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => openRow(row)}
                  aria-label={`Open ${row.subject || "(no subject)"} from ${row.sender}`}
                >
                  <Mail className="size-3.5 text-muted-foreground" />
                  <span className="min-w-0">
                    <span className="block truncate font-medium">
                      {row.subject || "(no subject)"}
                    </span>
                    <span className="block truncate text-2xs text-muted-foreground">
                      {row.sender} · {row.snippet}
                    </span>
                  </span>
                  <span className="font-mono text-2xs text-muted-foreground">{row.date_label}</span>
                </button>
              ))}
            </div>
          ) : (
            <div className="px-3 py-8 text-center text-xs text-muted-foreground">
              No quick matches. Press Enter to search all mail.
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border px-4 py-2 text-2xs text-muted-foreground">
          <span>↑/↓ select · Enter open · ⌘/Ctrl+Enter search all · Esc close</span>
          <Button
            type="button"
            variant="outline"
            size="xs"
            onClick={goToSearch}
            disabled={!query.trim()}
          >
            Search all mail
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
