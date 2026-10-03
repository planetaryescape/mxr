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
import { doneModeRequest, markModeDone } from "@/features/modes/modeDone";

import type { NowUpdatesCard } from "./api";
import { letGoPreview } from "./letGoCopy";

/**
 * Let go of the Updates card: the daemon's dry run of done in Updates for
 * the card's threads, then the same request for real. What stays in To do
 * is named before you confirm, and Undo puts it all back.
 */
export function LetGoDigestDialog({
  card,
  open,
  onOpenChange,
}: {
  card: NowUpdatesCard;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const preview = useQuery({
    queryKey: ["now-let-go-preview", card.thread_ids.join(",")],
    queryFn: () => doneModeRequest("updates", card.thread_ids, true),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
  });
  const copy = preview.data ? letGoPreview(card, preview.data.items) : null;
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent data-testid="let-go-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>{copy?.title ?? "Counting what to let go…"}</AlertDialogTitle>
          <AlertDialogDescription>
            {preview.isError
              ? `Preview failed: ${preview.error.message}`
              : (copy?.note ?? "Asking the daemon for the exact selection.")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          {copy && copy.threadIds.length > 0 ? (
            <AlertDialogAction
              onClick={() => {
                onOpenChange(false);
                void markModeDone("updates", copy.threadIds);
              }}
            >
              Let go
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
