import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Bookmark, BookmarkPlus, HelpCircle, Search, SearchX, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import {
  createSavedSearch,
  fetchSavedSearches,
  fetchSearch,
  fetchSearchGroups,
  searchGroupsKey,
  searchKey,
  type SearchGroupBy,
  type SearchMode,
  type SearchSort,
} from "./api";
import { SavedSearchManager } from "./SavedSearchManager";
import { KeyChip } from "@/components/KeyChip";
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
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { fetchAccounts } from "@/features/accounts/api";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { Centered } from "@/features/mailbox/MailViewParts";
import type { MessageGroupView } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { runReplaceableQuery } from "@/lib/requestCoordinator";
import { parseSearchTokens, removeSearchToken, searchSyntaxRows } from "@/lib/searchSyntax";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

const SEARCH_PAGE_LIMIT = 100;
type Scope = "threads" | "messages" | "attachments" | "triage";
type Verdict = "ACTION" | "FYI" | "ROUTINE";
const SEARCH_LENS = { kind: "search" } as const;

/**
 * Search results behave like a mailbox: same rows, keys and actions, and
 * a result opens beside the list at /search/<thread>?<query>, so Escape
 * comes back to the same results.
 */
export function SearchResultsRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const search = useSearch({ from: "/search" });
  const accountScope = useUiPrefs((s) => s.accountScope);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const q = search.q ?? "";
  const mode = search.mode ?? "lexical";
  const sort = search.sort ?? "relevance";
  const scope: Scope = (search.scope as Scope | undefined) ?? "threads";
  const verdict = search.verdict as Verdict | undefined;
  const groupBy = (search.groupBy as SearchGroupBy | undefined) ?? "from";
  const account = search.account ?? accountScope ?? undefined;
  const [draft, setDraft] = useState(q);
  const [saveOpen, setSaveOpen] = useState(false);
  const [manageOpen, setManageOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => setDraft(q), [q]);
  // Arriving with no query puts the cursor in the box.
  useEffect(() => {
    if (!q) inputRef.current?.focus();
  }, [q]);

  const request = { q, mode, sort, scope, account, limit: SEARCH_PAGE_LIMIT, verdict };
  const results = useInfiniteQuery({
    queryKey: searchKey(request),
    queryFn: ({ signal, pageParam }) =>
      runReplaceableQuery("search-results", signal, (combined) =>
        fetchSearch({ ...request, offset: pageParam }, { signal: combined }),
      ),
    initialPageParam: 0,
    getNextPageParam: (page) => (page.has_more ? (page.next_offset ?? undefined) : undefined),
    enabled: q.trim().length > 0,
  });
  const facets = useQuery({
    queryKey: searchGroupsKey({ q, mode, sort, scope, account, limit: 12, groupBy }),
    queryFn: ({ signal }) =>
      runReplaceableQuery("search-groups", signal, (combined) =>
        fetchSearchGroups(
          { q, mode, sort, scope, account, limit: 12, groupBy },
          { signal: combined },
        ),
      ),
    enabled: q.trim().length > 0,
  });
  const savedSearches = useQuery({
    queryKey: ["saved-searches"],
    queryFn: fetchSavedSearches,
    staleTime: 60_000,
  });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts, staleTime: 60_000 });

  const update = useCallback(
    (
      next: Partial<{
        q: string;
        mode: SearchMode;
        sort: SearchSort;
        scope: Scope;
        verdict: Verdict | null;
        groupBy: SearchGroupBy;
        account: string | null;
      }>,
    ) => {
      void navigate({
        to: "/search",
        search: {
          q: next.q ?? q,
          mode: next.mode ?? mode,
          sort: next.sort ?? sort,
          scope: next.scope ?? scope,
          verdict: next.verdict === null ? undefined : (next.verdict ?? verdict),
          groupBy: next.groupBy ?? groupBy,
          account: next.account === null ? undefined : (next.account ?? search.account),
        },
      });
    },
    [groupBy, mode, navigate, q, scope, search.account, sort, verdict],
  );

  const merged = useMemo(
    () => mergeGroups((results.data?.pages ?? []).flatMap((page) => page.groups)),
    [results.data],
  );
  const groups = useProjectedGroups(merged, SEARCH_LENS);
  const loaded = groups.reduce((sum, group) => sum + group.rows.length, 0);
  const total = results.data?.pages[0]?.total ?? loaded;
  const llmCalls = results.data?.pages[0]?.llm_calls;

  const save = useMutation({
    mutationFn: createSavedSearch,
    onSuccess: (_, input) => {
      toast.success(`Saved “${input.name}”`, {
        description: "It's in the sidebar under Saved searches.",
      });
      setSaveOpen(false);
      void qc.invalidateQueries({ queryKey: ["saved-searches"] });
      void qc.invalidateQueries({ queryKey: ["shell"] });
    },
    onError: (error) => toast.error("Couldn't save the search", { description: error.message }),
  });

  const tokens = parseSearchTokens(q);
  const realAccounts = (accounts.data?.accounts ?? []).filter((row) => row.enabled !== false);

  const toolbar = (
    <div className="pt-0.5">
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          update({ q: draft.trim() });
          inputRef.current?.blur();
          setActivePane("mailbox");
        }}
      >
        <div className="relative min-w-0 flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            ref={inputRef}
            aria-label="Search query"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.currentTarget.blur();
                setActivePane("mailbox");
              }
            }}
            placeholder="from:ada has:attachment invoice"
            className="h-10 pl-9 text-[14px]"
          />
        </div>
        <SyntaxHelp />
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-10"
          disabled={!q.trim()}
          onClick={() => setSaveOpen(true)}
        >
          <BookmarkPlus className="size-4" /> <span className="hidden @lg:inline">Save</span>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-10"
          aria-label="Manage saved searches"
          onClick={() => setManageOpen(true)}
        >
          <Bookmark className="size-4" />
        </Button>
      </form>
      {tokens.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {tokens.map((token) => (
            <button
              key={token.raw}
              type="button"
              onClick={() => update({ q: removeSearchToken(q, token) })}
              className={cn(
                "inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 font-mono text-[11.5px]",
                token.kind === "operator"
                  ? "border-primary/40 bg-primary-muted/50 text-foreground"
                  : "border-border text-muted-foreground",
              )}
              aria-label={`Remove ${token.label}`}
            >
              {token.label}
              <X className="size-3 opacity-60" />
            </button>
          ))}
        </div>
      ) : null}
      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        <Segmented
          label="Search mode"
          value={mode}
          options={[
            ["lexical", "Exact"],
            ["hybrid", "Hybrid"],
            ["semantic", "Meaning"],
          ]}
          onChange={(value) => update({ mode: value as SearchMode })}
        />
        <CompactSelect
          label="Show"
          value={scope}
          onChange={(value) =>
            update({
              scope: value as Scope,
              sort: value === "triage" ? "verdict" : sort === "verdict" ? "relevance" : sort,
            })
          }
          options={[
            ["threads", "Conversations"],
            ["messages", "Messages"],
            ["attachments", "Attachments"],
            ["triage", "Triage"],
          ]}
        />
        <CompactSelect
          label="Sort"
          value={sort}
          onChange={(value) => update({ sort: value as SearchSort })}
          options={[
            ["relevance", "Best match"],
            ["newest", "Newest"],
            ["oldest", "Oldest"],
            ...(scope === "triage" ? ([["verdict", "Verdict"]] as [string, string][]) : []),
          ]}
        />
        {scope === "triage" ? (
          <CompactSelect
            label="Verdict"
            value={verdict ?? "ALL"}
            onChange={(value) => update({ verdict: value === "ALL" ? null : (value as Verdict) })}
            options={[
              ["ALL", "Any"],
              ["ACTION", "Action"],
              ["FYI", "FYI"],
              ["ROUTINE", "Routine"],
            ]}
          />
        ) : null}
        {realAccounts.length > 1 ? (
          <CompactSelect
            label="Account"
            value={account ?? "ALL"}
            onChange={(value) => update({ account: value === "ALL" ? null : value })}
            options={[
              ["ALL", "All accounts"],
              ...realAccounts.map(
                (row) => [row.account_id, row.email || row.name] as [string, string],
              ),
            ]}
          />
        ) : null}
      </div>
      {q && (facets.data?.groups.length ?? 0) > 1 ? (
        <div className="mt-2.5 flex items-center gap-1.5 overflow-x-auto pb-0.5">
          <CompactSelect
            label="Narrow by"
            value={groupBy}
            onChange={(value) => update({ groupBy: value as SearchGroupBy })}
            options={[
              ["from", "Sender"],
              ["list", "List"],
              ["category", "Category"],
            ]}
          />
          {(facets.data?.groups ?? []).slice(0, 10).map((row) => (
            <button
              key={row.key}
              type="button"
              title={`${row.label}: ${plural(row.count, "message")}, ${row.unread} unread`}
              onClick={() =>
                update({
                  q: `${q} ${facetOperator(groupBy)}:${quoteIfNeeded(facetValue(row.key, row.label))}`.trim(),
                })
              }
              className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-border px-2.5 py-0.5 text-[12px] hover:border-primary/60 hover:bg-accent"
            >
              <span className="max-w-[16ch] truncate">{shortFacet(row.label)}</span>
              <span className="font-mono text-2xs text-muted-foreground">{row.count}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );

  return (
    <>
      <ListWithReader
        basePath="/search"
        preserveSearch
        title="Search"
        meta={
          q && results.isSuccess
            ? `${plural(total, "result")}${scope === "triage" && typeof llmCalls === "number" ? ` · ${plural(llmCalls, "LLM call")}` : ""}`
            : null
        }
        toolbar={toolbar}
        groups={groups}
        scopeKey={`search|${q}|${mode}|${scope}|${account ?? "all"}`}
        status={
          q.trim()
            ? results
            : { isLoading: false, isError: false, error: null, refetch: () => undefined }
        }
        hasMore={results.hasNextPage}
        loadingMore={results.isFetchingNextPage}
        onLoadMore={() => void results.fetchNextPage()}
        empty={
          q.trim() ? (
            <NoResults q={q} mode={mode} onTryHybrid={() => update({ mode: "hybrid" })} />
          ) : (
            <SearchStart
              saved={savedSearches.data?.searches ?? []}
              onRun={(query) => update({ q: query })}
            />
          )
        }
      />

      <Dialog open={saveOpen} onOpenChange={setSaveOpen}>
        <SaveSearchDialog
          q={q}
          pending={save.isPending}
          onCancel={() => setSaveOpen(false)}
          onSave={(name) => save.mutate({ name, query: q, mode })}
        />
      </Dialog>
      <Dialog open={manageOpen} onOpenChange={setManageOpen}>
        <DialogContent className="max-w-xl gap-0 p-0">
          <DialogHeader className="border-b border-border px-4 py-3">
            <DialogTitle>Saved searches</DialogTitle>
            <DialogDescription>
              Pinned ones lead the sidebar; g 1 to g 9 jump to them.
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-[60vh] overflow-auto">
            <SavedSearchManager
              searches={savedSearches.data?.searches ?? []}
              onChange={() => {
                void qc.invalidateQueries({ queryKey: ["saved-searches"] });
                void qc.invalidateQueries({ queryKey: ["shell"] });
              }}
              onRun={(saved) => {
                setManageOpen(false);
                update({
                  q: saved.query,
                  mode: (saved.search_mode as SearchMode | undefined) ?? mode,
                });
              }}
            />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

function SaveSearchDialog({
  q,
  pending,
  onCancel,
  onSave,
}: {
  q: string;
  pending: boolean;
  onCancel: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState("");
  return (
    <DialogContent className="max-w-md">
      <DialogHeader>
        <DialogTitle>Save this search</DialogTitle>
        <DialogDescription className="font-mono text-2xs">{q}</DialogDescription>
      </DialogHeader>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (name.trim()) onSave(name.trim());
        }}
      >
        <Input
          autoFocus
          aria-label="Name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="Invoices to file"
        />
        <DialogFooter className="mt-4">
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" disabled={!name.trim() || pending}>
            Save search
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}

function SearchStart({
  saved,
  onRun,
}: {
  saved: { id: string; name: string; query: string }[];
  onRun: (q: string) => void;
}) {
  return (
    <div className="flex-1 overflow-auto px-6 py-8">
      <div className="mx-auto max-w-xl">
        <h2 className="text-[15px] font-semibold">Search everything you have, offline</h2>
        <p className="mt-1 text-[13px] text-muted-foreground">
          Exact search is the default. Hybrid adds meaning-based matches when semantic search is on.
        </p>
        {saved.length > 0 ? (
          <>
            <h3 className="mb-1 mt-6 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
              Saved
            </h3>
            <ul className="divide-y divide-border/70 rounded-md border border-border">
              {saved.slice(0, 9).map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    onClick={() => onRun(item.query)}
                    className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-accent"
                  >
                    <span className="flex-1 truncate text-[13px]">{item.name}</span>
                    <span className="truncate font-mono text-2xs text-muted-foreground">
                      {item.query}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        ) : null}
        <h3 className="mb-1 mt-6 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
          Operators
        </h3>
        <dl className="grid grid-cols-[minmax(0,auto)_1fr] gap-x-4 gap-y-1 text-[13px]">
          {searchSyntaxRows.map(([operator, description]) => (
            <div key={operator} className="contents">
              <dt>
                <button
                  type="button"
                  onClick={() => onRun(operator)}
                  className="font-mono text-[12px] text-primary hover:underline"
                >
                  {operator}
                </button>
              </dt>
              <dd className="text-muted-foreground">{description}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}

function NoResults({
  q,
  mode,
  onTryHybrid,
}: {
  q: string;
  mode: SearchMode;
  onTryHybrid: () => void;
}) {
  const hasOperators = /\w+:/.test(q);
  return (
    <Centered
      icon={<SearchX className="size-6" />}
      title="No matches"
      body={
        hasOperators
          ? "Check the operators: from: and to: match addresses and names; is:unread and has:attachment need nothing after them."
          : mode === "lexical"
            ? "Exact search matches the words you typed. Hybrid also finds messages that mean the same thing."
            : "Nothing close enough. Try fewer or different words."
      }
      action={
        mode === "lexical" && !hasOperators ? (
          <Button variant="outline" size="sm" onClick={onTryHybrid}>
            Try hybrid search
          </Button>
        ) : undefined
      }
    />
  );
}

function Segmented({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: [string, string][];
  onChange: (value: string) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="flex rounded-md border border-border p-0.5"
    >
      {options.map(([id, text]) => (
        <button
          key={id}
          type="button"
          role="radio"
          aria-checked={value === id}
          onClick={() => onChange(id)}
          className={cn(
            "rounded px-2 py-0.5 text-[12.5px]",
            value === id
              ? "bg-accent font-medium text-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {text}
        </button>
      ))}
    </div>
  );
}

function CompactSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: [string, string][];
  onChange: (value: string) => void;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        aria-label={label}
        className="h-7 w-auto shrink-0 gap-1.5 border-border px-2 text-[12.5px]"
      >
        <span className="text-muted-foreground">{label}:</span>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map(([id, text]) => (
          <SelectItem key={id} value={id}>
            {text}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function SyntaxHelp() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-10"
          aria-label="Search operators"
        >
          <HelpCircle className="size-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <div className="mb-2 text-[13px] font-semibold">Operators</div>
        <div className="grid gap-1">
          {searchSyntaxRows.map(([operator, description]) => (
            <div key={operator} className="flex items-center justify-between gap-3 text-[12px]">
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono">{operator}</code>
              <span className="text-muted-foreground">{description}</span>
            </div>
          ))}
        </div>
        <p className="mt-3 text-2xs text-muted-foreground">
          Press <KeyChip>/</KeyChip> anywhere for quick search.
        </p>
      </PopoverContent>
    </Popover>
  );
}

function facetOperator(groupBy: SearchGroupBy): string {
  return groupBy === "from" ? "from" : groupBy === "list" ? "list" : "category";
}

/** Sender facets arrive as "Name <email>"; filter on the address. */
function facetValue(key: string, label: string): string {
  const email = label.match(/<([^>]+)>/)?.[1];
  return email ?? key;
}

function shortFacet(label: string): string {
  return label.replace(/\s*<[^>]+>/, "").trim() || label;
}

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}

function mergeGroups(groups: MessageGroupView[]): MessageGroupView[] {
  const merged = new Map<string, MessageGroupView>();
  const seen = new Set<string>();
  for (const group of groups) {
    const rows = group.rows.filter((row) => {
      const key = `${row.kind}:${row.kind === "thread" ? row.thread_id : row.id}:${row.attachment_id ?? ""}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
    const existing = merged.get(group.id);
    if (existing) existing.rows.push(...rows);
    else merged.set(group.id, { ...group, rows: [...rows] });
  }
  return [...merged.values()];
}
