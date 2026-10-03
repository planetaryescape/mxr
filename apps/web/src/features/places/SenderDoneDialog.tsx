import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";

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
import { doneModeRequest, markModeDone, type DoneMode } from "@/features/modes/modeDone";
import { plural } from "@/lib/format";

export interface SenderDone {
  mode: Extract<DoneMode, "updates" | "reading">;
  accountId: string;
  senderEmail: string;
  /** How the dialog names the sender. */
  label: string;
}

/**
 * Done here for everything one sender has in a mode: the daemon resolves
 * the threads (not just the ones loaded on screen) and its dry run gives
 * the count, so what you confirm is what runs.
 */
export function SenderDoneDialog({ target, onClose }: { target: SenderDone; onClose: () => void }) {
  const sender = { account_id: target.accountId, sender_email: target.senderEmail };
  const preview = useQuery({
    queryKey: ["sender-done-preview", target.mode, target.accountId, target.senderEmail],
    queryFn: () => doneModeRequest(target.mode, [], true, { sender }),
    staleTime: 0,
    gcTime: 0,
  });
  const going = preview.data?.items.filter((item) => !item.error) ?? [];
  const kept = going.filter((item) => (item.still_in ?? []).length > 0).length;
  const where = target.mode === "updates" ? "Updates" : "Reading";
  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent data-testid="sender-done-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {preview.data
              ? `Done with ${plural(going.length, "conversation")} from ${target.label}?`
              : "Counting what to let go…"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {preview.isError
              ? `Preview failed: ${preview.error.message}`
              : preview.data
                ? [
                    `They leave ${where} until ${target.label} writes again.`,
                    kept > 0
                      ? `${plural(kept, "conversation")} another mode still holds ${kept === 1 ? "stays" : "stay"} in the inbox.`
                      : null,
                    "Undo puts them back.",
                  ]
                    .filter(Boolean)
                    .join(" ")
                : "Asking the daemon for the exact selection."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          {going.length > 0 ? (
            <AlertDialogAction
              onClick={() => {
                onClose();
                void markModeDone(target.mode, [], { sender });
              }}
            >
              Done
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
