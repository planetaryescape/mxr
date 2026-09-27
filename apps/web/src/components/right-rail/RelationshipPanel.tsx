/* Relationship section of the sender panel: summary, style, drift and open
 * commitments with inline resolve. */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { resolveCommitment as resolveCommitmentApi } from "@/features/mailbox/api";
import { useModals } from "@/state/modalStore";
import { ProfileRow } from "./ProfileParts";
import { removeCommitmentFromSenderPayload, type RelationshipProfile } from "./railPayloads";
import { formatShortDate, styleSummary } from "./railFormat";

export function RelationshipPanel({
  payload,
  relationship,
}: {
  payload: unknown;
  relationship: RelationshipProfile;
}) {
  const commitments = relationship.open_commitments ?? [];
  const queryClient = useQueryClient();
  const openRail = useModals((state) => state.openRightRail);
  const resolveCommitment = useMutation({
    mutationFn: resolveCommitmentApi,
    onSuccess: (_result, commitmentId) => {
      openRail("sender-profile", removeCommitmentFromSenderPayload(payload, commitmentId));
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      toast.success("Commitment resolved");
    },
    onError: (error) => toast.error("Resolve failed", { description: error.message }),
  });
  return (
    <div className="space-y-3 rounded-md border border-border bg-muted/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-xs font-medium">Relationship</h4>
        {commitments.length > 0 ? <Badge variant="outline">{commitments.length} open</Badge> : null}
      </div>
      {relationship.drift ? (
        <div className="rounded-md border border-warning/40 bg-warning/10 px-2.5 py-2 text-2xs text-foreground">
          Voice drift: {relationship.drift.reason}
        </div>
      ) : null}
      {relationship.summary?.text ? (
        <p className="text-xs leading-relaxed text-muted-foreground">{relationship.summary.text}</p>
      ) : null}
      {relationship.summary?.known_topics?.length ? (
        <div className="flex flex-wrap gap-1">
          {relationship.summary.known_topics.slice(0, 12).map((topic) => (
            <Badge key={topic} variant="secondary" className="text-2xs">
              {topic}
            </Badge>
          ))}
        </div>
      ) : null}
      {relationship.style ? (
        <div className="grid gap-1.5">
          <ProfileRow label="Your style" value={styleSummary(relationship.style.formality_score)} />
          <ProfileRow
            label="Their style"
            value={styleSummary(relationship.style.formality_score_theirs)}
          />
          <ProfileRow
            label="Samples"
            value={`${relationship.style.msg_count_used} yours / ${relationship.style.msg_count_used_theirs} theirs`}
          />
        </div>
      ) : null}
      {commitments.length > 0 ? (
        <div className="space-y-1.5">
          {commitments.slice(0, 5).map((commitment) => (
            <div
              key={commitment.id}
              className="rounded border border-border/70 bg-background/50 px-2 py-1.5"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="font-medium">{commitment.what}</div>
                  <div className="mt-0.5 text-2xs text-muted-foreground">
                    {commitment.who_owes} · {commitment.direction}
                    {commitment.by_when ? ` · due ${formatShortDate(commitment.by_when)}` : ""}
                  </div>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 shrink-0 px-2 text-2xs"
                  disabled={resolveCommitment.isPending}
                  onClick={() => resolveCommitment.mutate(commitment.id)}
                >
                  <CheckCircle2 className="size-3" />
                  Resolve
                </Button>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
