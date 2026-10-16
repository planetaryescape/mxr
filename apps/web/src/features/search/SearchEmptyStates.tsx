/* What the results pane shows with no query yet, or no matches. */

import { Button } from "@/components/ui/button";
import { PageEmpty } from "@/components/PageParts";
import { searchSyntaxRows } from "@/lib/searchSyntax";
import type { SearchMode } from "./api";
import { SectionHeading } from "@/components/SectionHeading";

export function SearchStart({
  saved,
  onRun,
}: {
  saved: { id: string; name: string; query: string }[];
  onRun: (q: string) => void;
}) {
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <PageEmpty
        framed
        title="Search everything you have, offline"
        body="Exact search is the default. Hybrid adds meaning-based matches when semantic search is on."
        action={<SearchHelp saved={saved} onRun={onRun} />}
      />
    </div>
  );
}

function SearchHelp({
  saved,
  onRun,
}: {
  saved: { id: string; name: string; query: string }[];
  onRun: (q: string) => void;
}) {
  return (
    <div className="max-w-xl">
      {saved.length > 0 ? (
        <>
          <SectionHeading as="h3" className="mb-1 mt-6">
            Saved
          </SectionHeading>
          <ul className="divide-y divide-border/70 rounded-md border border-border">
            {saved.slice(0, 9).map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => onRun(item.query)}
                  className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-accent"
                >
                  <span className="flex-1 truncate text-sm">{item.name}</span>
                  <span className="truncate font-mono text-2xs text-muted-foreground">
                    {item.query}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : null}
      <SectionHeading as="h3" className="mb-1 mt-6">
        Operators
      </SectionHeading>
      <dl className="grid grid-cols-[minmax(0,auto)_1fr] gap-x-4 gap-y-1 text-sm">
        {searchSyntaxRows.map(([operator, description]) => (
          <div key={operator} className="contents">
            <dt>
              <button
                type="button"
                onClick={() => onRun(operator)}
                className="font-mono text-xs text-primary hover:underline"
              >
                {operator}
              </button>
            </dt>
            <dd className="text-muted-foreground">{description}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

export function NoResults({
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
    <PageEmpty
      framed
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
