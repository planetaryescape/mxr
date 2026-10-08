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
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

import { letGoRequest, type UpdateLine, type UpdateSetting } from "./api";
import { SETTING_LABEL, SETTINGS } from "./digestView";

/**
 * Let go of a digest: the daemon's dry run of exactly the cut, then the
 * same selection for real (its token makes the run refuse if the cut
 * changed meanwhile). What stays in To do is named before you confirm.
 */
export function LetGoAllDialog({
  open,
  onOpenChange,
  account,
  cut,
  source,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  account: string | null;
  /** The cut the digest showed. */
  cut: string;
  /** `e`: only this line's source, in its account. */
  source?: UpdateLine | null;
  onConfirm: (selectionToken: string) => void;
}) {
  const scope = source ? { account: source.account_id, sourceKey: source.source_key } : { account };
  const preview = useQuery({
    queryKey: ["updates-let-go-preview", scope.account ?? "all", cut, source?.source_key ?? ""],
    queryFn: () => letGoRequest({ ...scope, cut, dryRun: true }),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
  });
  const result = preview.data;
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent data-testid="updates-let-go-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {result ? `${result.line.replace(/\.$/, "")}?` : "Counting what to let go…"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {preview.isError
              ? `Preview failed: ${preview.error.message}`
              : result
                ? "Mail that arrived after the cut stays. Undo puts it all back."
                : "Asking the daemon for the exact selection."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          {result && result.message_count > 0 ? (
            <AlertDialogAction
              data-testid="updates-let-go-confirm"
              onClick={() => {
                onOpenChange(false);
                onConfirm(result.selection_token);
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

/** `K`: how this source reaches you. One choice, applied at once, with undo. */
export function TuneDialog({
  line,
  onOpenChange,
  onChoose,
}: {
  line: UpdateLine | null;
  onOpenChange: (open: boolean) => void;
  onChoose: (line: UpdateLine, setting: UpdateSetting) => void;
}) {
  return (
    <Dialog open={line !== null} onOpenChange={onOpenChange}>
      <DialogContent data-testid="updates-tune-dialog" className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Tune {line?.source_name}</DialogTitle>
          <DialogDescription>How this source reaches you. Undo puts it back.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-1.5">
          {SETTINGS.map((setting) => (
            <Button
              key={setting}
              variant="outline"
              className={cn(
                "justify-start",
                line?.setting === setting && "border-primary text-foreground",
              )}
              aria-pressed={line?.setting === setting}
              onClick={() => {
                if (!line) return;
                onOpenChange(false);
                onChoose(line, setting);
              }}
            >
              {SETTING_LABEL[setting]}
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
