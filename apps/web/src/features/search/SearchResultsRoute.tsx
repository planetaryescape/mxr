import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
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
} from "./api";
import { SavedSearchManager } from "./SavedSearchManager";
import { SaveSearchDialog } from "./SaveSearchDialog";
import { NoResults, SearchStart } from "./SearchEmptyStates";
import type { Scope, SearchUpdate, Verdict } from "./searchRouteParams";
import { SearchToolbar } from "./SearchToolbar";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { fetchAccounts } from "@/features/accounts/api";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import type { MessageGroupView } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { runReplaceableQuery } from "@/lib/requestCoordinator";
import { useUiPrefs } from "@/state/uiPrefsStore";

const SEARCH_PAGE_LIMIT = 100;
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
    (next: SearchUpdate) => {
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

  const realAccounts = (accounts.data?.accounts ?? []).filter((row) => row.enabled !== false);

  const toolbar = (
    <SearchToolbar
      q={q}
      draft={draft}
      setDraft={setDraft}
      inputRef={inputRef}
      mode={mode}
      sort={sort}
      scope={scope}
      verdict={verdict}
      groupBy={groupBy}
      account={account}
      realAccounts={realAccounts}
      facetGroups={facets.data?.groups}
      update={update}
      onSaveClick={() => setSaveOpen(true)}
      onManageClick={() => setManageOpen(true)}
    />
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
