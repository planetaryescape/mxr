import { useMutation, useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { useState } from "react";

import { fetchAccounts } from "@/features/accounts/api";
import { findExpert, type ExpertSuggestion } from "@/features/mailbox/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * Finds people in the local archive who have answered questions like the
 * one typed. Rendered in the right rail from a command-palette action.
 */
export function ExpertFinderPanel() {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountId = accounts.data?.accounts[0]?.account_id ?? "";
  const [query, setQuery] = useState("");
  const search = useMutation({
    mutationFn: (value: string) => findExpert({ accountId, query: value, limit: 8 }),
  });
  const experts: ExpertSuggestion[] = search.data?.experts ?? [];

  return (
    <div className="space-y-3 text-foreground">
      <div>
        <h3 className="text-sm font-semibold">Find an expert</h3>
        <p className="text-2xs text-muted-foreground">
          Who in your archive has answered questions like this before?
        </p>
      </div>
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          const value = query.trim();
          if (value && accountId) search.mutate(value);
        }}
      >
        <Input
          autoFocus
          aria-label="Expert question"
          placeholder="e.g. kubernetes ingress"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <Button
          type="submit"
          size="icon-sm"
          className="shrink-0"
          aria-label="Find experts"
          disabled={!query.trim() || !accountId || search.isPending}
        >
          <Search className="size-3" />
        </Button>
      </form>

      {!accountId ? (
        <div className="text-2xs text-muted-foreground">Connect an account first.</div>
      ) : search.isError ? (
        <div className="text-2xs text-destructive">{search.error.message}</div>
      ) : search.isPending ? (
        <div className="text-2xs text-muted-foreground">Searching…</div>
      ) : search.isSuccess && experts.length === 0 ? (
        <div className="text-2xs text-muted-foreground">No experts found for that question.</div>
      ) : (
        <div className="space-y-2">
          {experts.map((expert) => (
            <div
              key={expert.email}
              className="rounded-md border border-border bg-muted/30 p-3"
            >
              <div className="text-xs font-medium">{expert.display_name || expert.email}</div>
              <div className="break-all font-mono text-2xs text-muted-foreground">
                {expert.email}
              </div>
              <p className="mt-1 text-2xs text-muted-foreground">{expert.reason}</p>
              <div className="mt-1 text-2xs text-muted-foreground">
                {expert.answered_thread_count} thread
                {expert.answered_thread_count === 1 ? "" : "s"} answered
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
