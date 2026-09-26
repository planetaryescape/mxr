/*
 * "Needs attention": drafts the daemon started sending but never confirmed
 * (status `sending` with a stale heartbeat). Each can be reset back to an
 * editable draft, or sent again. The daemon refuses to send a draft still
 * marked `sending`, so Send resets first, and it always confirms: the first
 * attempt may have reached the recipient.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Loader2, RotateCcw, Send } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { formatRelativeAge } from "@/lib/utils";
import {
  fetchOrphanedDrafts,
  resetOrphanedDraft,
  sendStoredDraft,
  type OrphanedDraft,
} from "./api";

export const orphanedDraftsQueryKey = ["drafts", "orphaned"] as const;

export function OrphanedDrafts() {
  const queryClient = useQueryClient();
  const orphans = useQuery({ queryKey: orphanedDraftsQueryKey, queryFn: fetchOrphanedDrafts });
  const [confirming, setConfirming] = useState<OrphanedDraft | null>(null);

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["drafts"] });
  const reset = useMutation({
    mutationFn: (draft: OrphanedDraft) => resetOrphanedDraft(draft.id),
    onSuccess: () => {
      toast.success("Draft reset", { description: "It is back in your drafts to edit or send." });
      void refresh();
    },
    onError: (error) => toast.error("Reset failed", { description: error.message }),
  });
  const send = useMutation({
    mutationFn: async (draft: OrphanedDraft) => {
      await resetOrphanedDraft(draft.id);
      await sendStoredDraft(draft.id);
    },
    onSuccess: () => {
      toast.success("Message sent");
      void refresh();
    },
    // A refused send (safety block, provider error) leaves the draft reset,
    // so it now shows in the list below for editing.
    onError: (error) => {
      toast.error("Send failed", { description: error.message });
      void refresh();
    },
  });

  const rows = orphans.data ?? [];
  if (rows.length === 0) return null;
  const pending = reset.isPending || send.isPending;

  return (
    <section aria-labelledby="orphaned-drafts-heading" className="border-b border-border">
      <div className="flex items-center gap-2 px-6 pt-4 pb-2">
        <AlertTriangle className="size-3.5 text-warning" aria-hidden="true" />
        <h2 id="orphaned-drafts-heading" className="text-xs font-semibold">
          Needs attention
        </h2>
        <span className="text-2xs text-muted-foreground">
          mxr started sending these but never confirmed delivery.
        </span>
      </div>
      <ul className="divide-y divide-border">
        {rows.map((draft) => {
          const subject = draft.subject.trim() || "(no subject)";
          return (
            <li key={draft.id} className="flex items-center gap-3 px-6 py-3">
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">{subject}</div>
                <div className="mt-0.5 truncate text-2xs text-muted-foreground">
                  {formatRecipients(draft)} · stuck since{" "}
                  {formatRelativeAge(new Date(draft.updated_at))} ago
                </div>
              </div>
              <Button
                variant="outline"
                size="sm"
                disabled={pending}
                aria-label={`Reset ${subject}`}
                onClick={() => reset.mutate(draft)}
              >
                <RotateCcw className="size-3.5" />
                Reset
              </Button>
              <Button
                size="sm"
                disabled={pending}
                aria-label={`Send ${subject}`}
                onClick={() => setConfirming(draft)}
              >
                <Send className="size-3.5" />
                Send
              </Button>
            </li>
          );
        })}
      </ul>

      <AlertDialog open={confirming !== null} onOpenChange={(open) => !open && setConfirming(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Send this draft again?</AlertDialogTitle>
            <AlertDialogDescription>
              To {confirming ? formatRecipients(confirming) : ""}. The first attempt may already
              have been delivered, so check Sent before sending again.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel variant="outline">Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={send.isPending}
              onClick={() => {
                if (confirming) send.mutate(confirming);
                setConfirming(null);
              }}
            >
              {send.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Send className="size-3" />
              )}
              Send now
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}

function formatRecipients(draft: OrphanedDraft): string {
  if (draft.to.length === 0) return "no recipients";
  return draft.to.map((address) => address.name || address.email).join(", ");
}
