import { selectedWorkspace } from "@/lib/workspaceOwner";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { fetchOrphanedDrafts } from "./api";
import {
  fetchSendOutcome,
  readSendReferences,
  recoverSendOutcome,
  type SendReference,
} from "./sendOutcomes";

export const orphanedDraftsQueryKey = ["drafts", "orphaned"] as const;

function OutcomeRow({ reference }: { reference: SendReference }) {
  const [preview, setPreview] = useState<{ evidence?: string; reason: string } | null>(null);
  const [recovering, setRecovering] = useState(false);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const outcome = useQuery({
    queryKey: ["send-outcome", reference.binding, reference.accountId, reference.operationId],
    queryFn: () => fetchSendOutcome(reference.accountId, reference.operationId, reference.binding),
    refetchInterval: (query) =>
      ["waiting", "unknown"].includes(query.state.data?.outcome?.state ?? "") ? 30_000 : false,
  });
  const state = outcome.data?.outcome?.state;
  async function recover(evidence?: string) {
    setRecovering(true);
    setRecoveryError(null);
    try {
      const operationId = outcome.data?.outcome?.identity.operation_id ?? reference.operationId;
      const result = await recoverSendOutcome({ ...reference, operationId }, evidence);
      if (evidence) {
        setPreview(null);
        await outcome.refetch();
      } else
        setPreview({
          evidence: result.recoverable ? (result.evidence_sha256 ?? undefined) : undefined,
          reason: result.reason ?? "No definite non-submission evidence",
        });
    } catch (error) {
      setRecoveryError(
        error instanceof Error
          ? error.message
          : "Recovery failed; the original operation remains protected.",
      );
    } finally {
      setRecovering(false);
    }
  }
  const description = outcome.isPending
    ? "Checking send outcome…"
    : outcome.isError
      ? "Could not check the send outcome. Check again before deciding whether to send."
      : state === "waiting"
        ? `Waiting for the mail provider; retrying automatically${outcome.data?.outcome?.recovery?.retry_at ? ` at ${outcome.data.outcome.recovery.retry_at}` : ""}. Do not resend.`
        : state === "accepted"
          ? "Provider accepted this send."
          : state === "not_accepted"
            ? "Provider did not accept this send. Review the draft before a new attempt."
            : state === "unknown"
              ? "Acceptance is uncertain. Do not resend."
              : "No submission was claimed for this operation. Review the saved draft before sending.";
  return (
    <li className="flex items-center gap-3 border-b border-border/60 px-2 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">Send {reference.draftId}</div>
        <div className="text-2xs text-muted-foreground">{description}</div>
        <div className="truncate font-mono text-2xs">
          {outcome.data?.outcome?.identity.operation_id ?? reference.operationId}
        </div>
        {outcome.error && <div role="alert">{outcome.error.message}</div>}
        {state === "not_accepted" && outcome.data?.outcome?.recovery?.reason ? (
          <div className="text-2xs">
            Action required: {outcome.data.outcome.recovery.reason.replaceAll("_", " ")}. Review the
            draft and account before sending again.
          </div>
        ) : null}
        {recoveryError ? <div role="alert">{recoveryError}</div> : null}
        {preview ? (
          <div className="text-2xs">Recovery preview: {preview.reason}. No mail sent.</div>
        ) : null}
      </div>
      <Button
        variant="outline"
        size="sm"
        disabled={outcome.isFetching}
        onClick={() => void outcome.refetch()}
      >
        Check outcome
      </Button>
      {state === "unknown" && !outcome.data?.outcome?.recovery ? (
        preview?.evidence ? (
          <Button
            variant="outline"
            size="sm"
            disabled={recovering}
            onClick={() => void recover(preview.evidence)}
          >
            Resume original approved send
          </Button>
        ) : (
          <Button variant="outline" size="sm" disabled={recovering} onClick={() => void recover()}>
            Preview recovery
          </Button>
        )
      ) : null}
    </li>
  );
}

export function OrphanedDrafts() {
  const orphans = useQuery({ queryKey: orphanedDraftsQueryKey, queryFn: fetchOrphanedDrafts });
  const references = readSendReferences();
  for (const draft of orphans.data ?? []) {
    if (!references.some((reference) => reference.draftId === draft.id)) {
      references.push({
        binding: selectedWorkspace(),
        accountId: draft.account_id,
        draftId: draft.id,
        operationId: `draft:${draft.id}`,
      });
    }
  }
  if (references.length === 0) return null;
  return (
    <section aria-labelledby="orphaned-drafts-heading" className="mb-8">
      <div className="mb-2 flex items-center gap-2 border-b border-border pb-1.5">
        <AlertTriangle className="size-3.5 text-warning" aria-hidden="true" />
        <h2
          id="orphaned-drafts-heading"
          className="font-mono text-2xs uppercase tracking-[0.12em] text-warning"
        >
          Send outcomes
        </h2>
      </div>
      <ul>
        {references.map((reference) => (
          <OutcomeRow key={reference.operationId} reference={reference} />
        ))}
      </ul>
    </section>
  );
}
