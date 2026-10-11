import { useQuery } from "@tanstack/react-query";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { fetchOrphanedDrafts } from "./api";
import { fetchSendOutcome, readSendReferences, type SendReference } from "./sendOutcomes";

export const orphanedDraftsQueryKey = ["drafts", "orphaned"] as const;

function OutcomeRow({ reference }: { reference: SendReference }) {
  const outcome = useQuery({
    queryKey: ["send-outcome", reference.accountId, reference.operationId],
    queryFn: () => fetchSendOutcome(reference.accountId, reference.operationId),
  });
  const state = outcome.data?.outcome?.state;
  return <li className="flex items-center gap-3 border-b border-border/60 px-2 py-2.5">
    <div className="min-w-0 flex-1">
      <div className="truncate text-[13px] font-medium">Send {reference.draftId}</div>
      <div className="text-2xs text-muted-foreground">
        {state === "accepted" ? "Provider accepted this send." : state === "not_accepted" ? "Provider did not accept this send. Review the draft before a new attempt." : "Acceptance is uncertain. Do not resend."}
      </div>
      <div className="truncate font-mono text-2xs">{outcome.data?.outcome?.identity.operation_id ?? reference.operationId}</div>
      {outcome.error && <div role="alert">{outcome.error.message}</div>}
    </div>
    <Button variant="outline" size="sm" disabled={outcome.isFetching} onClick={() => void outcome.refetch()}>Check outcome</Button>
  </li>;
}

export function OrphanedDrafts() {
  const orphans = useQuery({ queryKey: orphanedDraftsQueryKey, queryFn: fetchOrphanedDrafts });
  const references = readSendReferences();
  for (const draft of orphans.data ?? []) {
    if (!references.some((reference) => reference.draftId === draft.id)) {
      references.push({ accountId: draft.account_id, draftId: draft.id, operationId: `draft:${draft.id}` });
    }
  }
  if (references.length === 0) return null;
  return <section aria-labelledby="orphaned-drafts-heading" className="mb-8">
    <div className="mb-2 flex items-center gap-2 border-b border-border pb-1.5">
      <AlertTriangle className="size-3.5 text-warning" aria-hidden="true" />
      <h2 id="orphaned-drafts-heading" className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-warning">Send outcomes</h2>
    </div>
    <ul>{references.map((reference) => <OutcomeRow key={reference.operationId} reference={reference} />)}</ul>
  </section>;
}
