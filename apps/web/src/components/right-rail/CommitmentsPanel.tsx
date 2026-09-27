/* Open commitments list with resolve. */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { resolveCommitment as resolveCommitmentApi } from "@/features/mailbox/api";
import { useModals } from "@/state/modalStore";
import { extractCommitments } from "./railPayloads";
import { formatShortDate } from "./railFormat";

export function CommitmentsPanel({ payload }: { payload: unknown }) {
  const commitments = extractCommitments(payload);
  const queryClient = useQueryClient();
  const openRail = useModals((state) => state.openRightRail);
  const resolveCommitment = useMutation({
    mutationFn: resolveCommitmentApi,
    onSuccess: (_result, commitmentId) => {
      openRail("commitments", {
        commitments: commitments.filter((item) => item.id !== commitmentId),
      });
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      toast.success("Commitment resolved");
    },
    onError: (error) => toast.error("Resolve failed", { description: error.message }),
  });

  if (commitments.length === 0) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-4 text-sm text-foreground">
        No open commitments.
      </div>
    );
  }

  return (
    <div className="space-y-2 text-foreground">
      <h3 className="text-sm font-semibold">Open commitments</h3>
      {commitments.map((commitment) => (
        <div key={commitment.id} className="rounded-md border border-border bg-muted/30 p-3">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0 space-y-1">
              <div className="text-xs font-medium">{commitment.what}</div>
              <div className="text-2xs text-muted-foreground">
                {commitment.who_owes} · {commitment.direction}
                {commitment.email ? ` · ${commitment.email}` : ""}
                {commitment.by_when ? ` · due ${formatShortDate(commitment.by_when)}` : ""}
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 shrink-0 px-2 text-2xs"
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
  );
}
