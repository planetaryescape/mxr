import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { BookmarkPlus, HelpCircle, RefreshCw, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import {
  createSavedSearch,
  fetchSavedSearches,
  fetchSearch,
  searchKey,
  type SearchMode,
  type SearchSort,
} from "./api";
import type { MessageRowView } from "@/features/mailbox/types";
import { fetchThread } from "@/features/mailbox/api";
import { EmptyState } from "@/components/EmptyState";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { runReplaceableQuery } from "@/lib/requestCoordinator";
import { parseSearchTokens, removeSearchToken, searchSyntaxRows } from "@/lib/searchSyntax";

export function SearchResultsRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const search = useSearch({ from: "/search" });
  const q = search.q ?? "";
  const mode = search.mode ?? "lexical";
  const sort = search.sort ?? "relevance";
  const [saveOpen, setSaveOpen] = useState(false);
  const [saveName, setSaveName] = useState("");
  const [previewRowId, setPreviewRowId] = useState<string | null>(null);
  const [draftQ, setDraftQ] = useState(q);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useQuery({
    queryKey: searchKey({ q, mode, sort, account: search.account, limit: 100 }),
    queryFn: ({ signal }) =>
      runReplaceableQuery("search-results", signal, (combinedSignal) =>
        fetchSearch(
          { q, mode, sort, account: search.account, limit: 100 },
          { signal: combinedSignal },
        ),
      ),
    enabled: q.trim().length > 0,
  });
  const savedSearches = useQuery({
    queryKey: ["saved-searches"],
    queryFn: fetchSavedSearches,
    staleTime: 60_000,
  });
  const saveSearch = useMutation({
    mutationFn: createSavedSearch,
    onSuccess: () => {
      toast.success("Saved search created");
      setSaveOpen(false);
      setSaveName("");
      void qc.invalidateQueries({ queryKey: ["saved-searches"] });
      void qc.invalidateQueries({ queryKey: ["shell"] });
    },
    onError: (error) => toast.error("Save search failed", { description: error.message }),
  });

  function updateSearch(next: { q?: string; mode?: SearchMode; sort?: SearchSort }) {
    void navigate({
      to: "/search",
      search: {
        q: next.q ?? q,
        mode: next.mode ?? mode,
        sort: next.sort ?? sort,
        account: search.account,
      },
    });
  }

  const tokens = parseSearchTokens(q);
  const rows = useMemo(
    () => results.data?.groups.flatMap((group) => group.rows) ?? [],
    [results.data?.groups],
  );

  const openPreviewRow = useCallback(() => {
    const row = rows.find((candidate) => candidate.id === previewRowId) ?? rows[0];
    if (!row) return;
    void navigate({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "inbox", threadId: row.thread_id },
    });
  }, [navigate, previewRowId, rows]);

  const movePreview = useCallback(
    (delta: number) => {
      if (rows.length === 0) return;
      setPreviewRowId((current) => {
        const currentIndex = current ? rows.findIndex((row) => row.id === current) : -1;
        const fallback = delta > 0 ? -1 : rows.length;
        const nextIndex = Math.max(
          0,
          Math.min(rows.length - 1, (currentIndex >= 0 ? currentIndex : fallback) + delta),
        );
        return rows[nextIndex]?.id ?? null;
      });
    },
    [rows],
  );
  const previewRow = rows.find((row) => row.id === previewRowId) ?? rows[0];
  const preview = useQuery({
    queryKey: ["search-preview", previewRow?.thread_id],
    queryFn: () => fetchThread(previewRow?.thread_id ?? ""),
    enabled: Boolean(previewRow?.thread_id),
    staleTime: 30_000,
  });

  useEffect(() => {
    if (rows.length === 0) {
      setPreviewRowId(null);
      return;
    }
    setPreviewRowId((current) =>
      current && rows.some((row) => row.id === current) ? current : (rows[0]?.id ?? null),
    );
  }, [rows]);

  useEffect(() => {
    setDraftQ(q);
  }, [q]);

  useEffect(() => {
    if (!previewRowId) return;
    document
      .getElementById(`search-result-${previewRowId}`)
      ?.scrollIntoView?.({ block: "nearest" });
  }, [previewRowId]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (saveOpen || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
        return;
      }
      const target = event.target;
      if (target instanceof HTMLElement) {
        if (target.closest("input, textarea, select, [contenteditable=true]")) return;
        if (
          target.closest("button, a, [role=button]") &&
          !target.closest("[data-search-result-row]")
        ) {
          return;
        }
      }
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        movePreview(1);
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        movePreview(-1);
      } else if (event.key === "Enter" || event.key === "o") {
        event.preventDefault();
        openPreviewRow();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [movePreview, openPreviewRow, saveOpen]);

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="flex flex-wrap items-end gap-3">
          <div className="min-w-[280px] flex-1">
            <Label htmlFor="search-page-input">Search query</Label>
            <form
              className="mt-1 flex gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                updateSearch({ q: draftQ });
                inputRef.current?.blur();
              }}
            >
              <Input
                ref={inputRef}
                id="search-page-input"
                name="q"
                value={draftQ}
                onChange={(event) => setDraftQ(event.target.value)}
                placeholder="from:alice has:attachment"
                className="h-9 bg-input text-sm"
              />
              <Button type="submit">
                <Search className="size-3" />
                Search
              </Button>
            </form>
          </div>
          <div>
            <Label>Mode</Label>
            <ToggleGroup
              className="mt-1"
              type="single"
              value={mode}
              onValueChange={(value) => {
                if (value) updateSearch({ mode: value as SearchMode });
              }}
              aria-label="Search mode"
            >
              <ToggleGroupItem value="lexical" size="sm">
                Lexical
              </ToggleGroupItem>
              <ToggleGroupItem value="semantic" size="sm">
                Semantic
              </ToggleGroupItem>
              <ToggleGroupItem value="hybrid" size="sm">
                Hybrid
              </ToggleGroupItem>
            </ToggleGroup>
          </div>
          <div className="w-32">
            <Label>Sort</Label>
            <Select
              value={sort}
              onValueChange={(value) => updateSearch({ sort: value as SearchSort })}
            >
              <SelectTrigger className="mt-1 h-9" aria-label="Search sort">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="relevance">Relevance</SelectItem>
                <SelectItem value="newest">Newest</SelectItem>
                <SelectItem value="oldest">Oldest</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Button variant="outline" onClick={() => setSaveOpen(true)} disabled={!q.trim()}>
            <BookmarkPlus className="size-3" />
            Save
          </Button>
          <SyntaxHelp />
        </div>
        {tokens.length > 0 ? (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {tokens.map((token) => (
              <Badge
                key={token.raw}
                asChild
                variant="outline"
                className="py-1 text-foreground hover:bg-muted"
              >
                <button
                  type="button"
                  onClick={() => updateSearch({ q: removeSearchToken(q, token) })}
                >
                  {token.label}
                  <X className="size-3 text-muted-foreground" />
                </button>
              </Badge>
            ))}
          </div>
        ) : null}
      </header>

      {!q.trim() ? (
        <EmptyState
          icon={Search}
          title="Search local mail"
          description="Use Gmail-style operators or plain text. Exact lexical search stays the default path."
        />
      ) : results.isLoading ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 10 }, (_, index) => (
            <div key={index} className="h-14 animate-pulse rounded-md bg-muted" />
          ))}
        </div>
      ) : results.isError ? (
        <EmptyState
          icon={RefreshCw}
          title="Search failed"
          description={results.error.message}
          action={<Button onClick={() => results.refetch()}>Retry</Button>}
        />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={Search}
          title="No matches"
          description="Try a broader query or switch search mode."
        />
      ) : (
        <div className="grid min-h-0 flex-1 gap-3 overflow-hidden p-3 xl:grid-cols-[minmax(0,1fr)_420px]">
          <div className="mb-2 flex items-center justify-between px-1 text-2xs text-muted-foreground">
            <span>
              {results.data?.total ?? rows.length} results · {mode} · {sort}
            </span>
            <span>{savedSearches.data?.searches.length ?? 0} saved searches</span>
          </div>
          <div className="min-h-0 overflow-auto rounded-lg border border-border bg-surface xl:row-start-2">
            {rows.map((row) => (
              <SearchResultRow
                key={row.id}
                row={row}
                active={previewRow?.id === row.id}
                onPreview={() => setPreviewRowId(row.id)}
              />
            ))}
          </div>
          <SearchPreviewPane
            row={previewRow}
            loading={preview.isLoading}
            error={preview.error}
            thread={preview.data}
          />
        </div>
      )}

      <Dialog open={saveOpen} onOpenChange={setSaveOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Save search</DialogTitle>
            <DialogDescription>
              Saved searches become reusable lenses in the sidebar when the daemon shell exposes
              them.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="saved-search-name">Name</Label>
            <Input
              id="saved-search-name"
              value={saveName}
              onChange={(event) => setSaveName(event.target.value)}
              placeholder="Invoices from Alice"
            />
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setSaveOpen(false)}>
              Cancel
            </Button>
            <Button
              disabled={!saveName.trim() || saveSearch.isPending}
              onClick={() => saveSearch.mutate({ name: saveName.trim(), query: q, mode })}
            >
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function SearchResultRow({
  row,
  active,
  onPreview,
}: {
  row: MessageRowView;
  active: boolean;
  onPreview: () => void;
}) {
  const navigate = useNavigate();
  return (
    <button
      id={`search-result-${row.id}`}
      data-search-result-row="true"
      type="button"
      className={`grid w-full grid-cols-[minmax(120px,190px)_1fr_auto] gap-3 border-b border-border px-4 py-3 text-left last:border-b-0 hover:bg-muted ${active ? "bg-muted" : ""}`}
      aria-current={active ? "true" : undefined}
      onFocus={onPreview}
      onMouseEnter={onPreview}
      onClick={() =>
        navigate({
          to: "/m/$mailbox/$threadId",
          params: { mailbox: "inbox", threadId: row.thread_id },
        })
      }
    >
      <div className="truncate text-xs text-muted-foreground">{row.sender}</div>
      <div className="min-w-0">
        <div className="truncate text-xs font-medium">{row.subject || "(no subject)"}</div>
        <div className="truncate text-2xs text-muted-foreground">{row.snippet}</div>
      </div>
      <div className="font-mono text-2xs text-muted-foreground">{row.date_label}</div>
    </button>
  );
}

function SearchPreviewPane({
  row,
  loading,
  error,
  thread,
}: {
  row?: MessageRowView;
  loading: boolean;
  error: Error | null;
  thread?: Awaited<ReturnType<typeof fetchThread>>;
}) {
  if (!row) return null;
  const previewText =
    thread?.bodies
      .map((body) => body.reader_text ?? body.text_plain)
      .filter(Boolean)
      .join("\n\n") || row.snippet;
  return (
    <aside className="hidden min-h-0 rounded-lg border border-border bg-surface xl:row-span-2 xl:block">
      <div className="border-b border-border px-4 py-3">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Preview
        </div>
        <h2 className="mt-1 line-clamp-2 text-sm font-semibold">
          {(thread?.thread.subject ?? row.subject) || "(no subject)"}
        </h2>
        <div className="mt-1 truncate text-2xs text-muted-foreground">
          {row.sender} · {row.date_label}
        </div>
      </div>
      <div className="max-h-[calc(100vh-260px)] overflow-auto p-4 text-xs leading-6 text-foreground">
        {loading ? (
          <div className="text-muted-foreground">Loading preview...</div>
        ) : error ? (
          <div className="text-destructive">{error.message}</div>
        ) : (
          <pre className="whitespace-pre-wrap break-words font-sans">{previewText}</pre>
        )}
      </div>
    </aside>
  );
}

function SyntaxHelp() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="Search syntax">
          <HelpCircle className="size-3" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <div className="mb-2 text-xs font-semibold">Search operators</div>
        <div className="grid gap-1">
          {searchSyntaxRows.map(([operator, description]) => (
            <div key={operator} className="flex items-center justify-between gap-3 text-2xs">
              <code className="rounded bg-muted px-1.5 py-0.5">{operator}</code>
              <span className="text-muted-foreground">{description}</span>
            </div>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
