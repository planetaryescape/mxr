/*
 * Search box, operator chips, mode/scope/sort/verdict/account pickers and
 * the "narrow by" facet chips above the results list.
 */

import { Bookmark, BookmarkPlus, Search, X } from "lucide-react";
import type { RefObject } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { RuntimeAccount } from "@/features/compose/api";
import { plural } from "@/lib/format";
import { parseSearchTokens, removeSearchToken } from "@/lib/searchSyntax";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import type { SearchAggregationRow, SearchGroupBy, SearchMode, SearchSort } from "./api";
import { CompactSelect, Segmented } from "./SearchControls";
import { facetOperator, facetValue, quoteIfNeeded, shortFacet } from "./searchFacets";
import type { Scope, SearchUpdate, Verdict } from "./searchRouteParams";
import { SyntaxHelp } from "./SyntaxHelp";

export function SearchToolbar({
  q,
  draft,
  setDraft,
  inputRef,
  mode,
  sort,
  scope,
  verdict,
  groupBy,
  account,
  realAccounts,
  facetGroups,
  update,
  onSaveClick,
  onManageClick,
}: {
  q: string;
  draft: string;
  setDraft: (value: string) => void;
  inputRef: RefObject<HTMLInputElement | null>;
  mode: SearchMode;
  sort: SearchSort;
  scope: Scope;
  verdict: Verdict | undefined;
  groupBy: SearchGroupBy;
  account: string | undefined;
  realAccounts: RuntimeAccount[];
  facetGroups: SearchAggregationRow[] | undefined;
  update: (next: SearchUpdate) => void;
  onSaveClick: () => void;
  onManageClick: () => void;
}) {
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const tokens = parseSearchTokens(q);

  return (
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
          onClick={onSaveClick}
        >
          <BookmarkPlus className="size-4" /> <span className="hidden @lg:inline">Save</span>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-10"
          aria-label="Manage saved searches"
          onClick={onManageClick}
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
      {q && (facetGroups?.length ?? 0) > 1 ? (
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
          {(facetGroups ?? []).slice(0, 10).map((row) => (
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
}
