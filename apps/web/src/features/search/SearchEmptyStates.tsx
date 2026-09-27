/* What the results pane shows with no query yet, or no matches. */

import { SearchX } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Centered } from "@/features/mailbox/MailViewParts";
import { searchSyntaxRows } from "@/lib/searchSyntax";
import type { SearchMode } from "./api";

export function SearchStart({
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
