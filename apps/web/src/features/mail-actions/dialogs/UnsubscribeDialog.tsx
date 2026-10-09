import { useQuery } from "@tanstack/react-query";
import { Archive, Loader2, MailX } from "lucide-react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { UNDO_TOAST_DURATION_MS } from "@/components/ui/sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { unsubscribeAndClearSender, unsubscribeFromSender } from "@/features/mailbox/api";
import { parseAddress, plural } from "@/lib/format";
import { useUndo } from "@/state/undoStore";

import { invalidateMailQueries, performUndo } from "../mailMutations";
import type { MailTarget } from "../target";

/**
 * TUI `D`: unsubscribe only (Enter or u), or unsubscribe and archive
 * everything from the sender (a). The archive count comes from a dry run
 * of the same daemon request, so the number shown is the number archived.
 */
export function UnsubscribeDialog({
  target,
  onDone,
  onClose,
}: {
  target: MailTarget;
  onDone?: () => void;
  onClose: () => void;
}) {
  const sender = parseAddress(target.primary?.sender_detail ?? target.primary?.sender);
  const address = sender.email ?? "";
  const preview = useQuery({
    queryKey: ["unsubscribe-preview", address, target.accountId],
    queryFn: () =>
      unsubscribeAndClearSender({ address, accountId: target.accountId, dryRun: true }),
    enabled: Boolean(address),
    staleTime: 0,
  });
  const result = preview.data?.result;
  const count = result?.message_count ?? 0;
  const noMethod = result?.status === "no_method" || result?.method === "None";
  const how = describeMethod(result?.method);

  const unsubscribeOnly = () => {
    if (!target.primary) return;
    onClose();
    unsubscribeFromSender({ messageId: target.primary.id, archive: false })
      .then(() => toast.success(`Unsubscribed from ${sender.name ?? address}`))
      .catch((error: Error) => toast.error("Unsubscribe failed", { description: error.message }));
  };

  const unsubscribeAndArchive = () => {
    onClose();
    onDone?.();
    unsubscribeAndClearSender({ address, accountId: target.accountId, archiveOnNoMethod: noMethod })
      .then((response) => {
        const outcome = response.result;
        if (!response.ok || !outcome) {
          toast.error("Unsubscribe failed", {
            description: outcome?.error ?? "No result from the daemon",
          });
          return;
        }
        const mutationId = outcome.mutation_id;
        const undo = mutationId
          ? async () => {
              useUndo.getState().retireUndo(undo!);
              return performUndo(mutationId);
            }
          : null;
        if (undo) useUndo.getState().recordUndo(undo, mutationId ?? undefined);
        else useUndo.getState().recordNoUndo();
        toast.success(`Unsubscribed and archived ${plural(outcome.archived_count, "message")}`, {
          duration: undo ? UNDO_TOAST_DURATION_MS : 6000,
          action: undo ? { label: "Undo archive", onClick: () => void undo() } : undefined,
        });
        void invalidateMailQueries();
      })
      .catch((error: Error) => toast.error("Unsubscribe failed", { description: error.message }));
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-md"
        onKeyDown={(event) => {
          // Enter on a focused option is that option's click, not a shortcut.
          if (event.key === "Enter" && event.target instanceof HTMLButtonElement) return;
          if (event.key === "u" || event.key === "Enter") {
            event.preventDefault();
            unsubscribeOnly();
          } else if (event.key === "a" && !preview.isLoading) {
            event.preventDefault();
            unsubscribeAndArchive();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>Unsubscribe from {sender.name ?? address}?</DialogTitle>
          <DialogDescription className="font-mono text-2xs">{address}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <button
            type="button"
            onClick={unsubscribeOnly}
            className="flex items-center gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:border-primary/60 hover:bg-accent"
          >
            <MailX className="size-4 text-muted-foreground" />
            <span className="flex-1">
              <span className="block text-[13px] font-medium">Unsubscribe</span>
              <span className="block text-2xs text-muted-foreground">
                {how ? `${how}. ` : ""}Keep the mail you already have
              </span>
            </span>
            <KeyChip>u</KeyChip>
          </button>
          <button
            type="button"
            onClick={unsubscribeAndArchive}
            disabled={preview.isLoading || !address}
            className="flex items-center gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:border-primary/60 hover:bg-accent disabled:opacity-60"
          >
            {preview.isLoading ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : (
              <Archive className="size-4 text-muted-foreground" />
            )}
            <span className="flex-1">
              <span className="block text-[13px] font-medium">
                Unsubscribe and archive{" "}
                {preview.isLoading ? "their mail" : plural(count, "message")}
              </span>
              <span className="block text-2xs text-muted-foreground">
                {preview.isError
                  ? `Preview failed: ${preview.error.message}`
                  : noMethod
                    ? "No unsubscribe link found; this only archives"
                    : "Everything from this sender leaves the inbox; undo for a minute"}
              </span>
            </span>
            <KeyChip>a</KeyChip>
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** The daemon's UnsubscribeMethod, externally tagged: "None" or { Kind: {...} }. */
function describeMethod(method: unknown): string | null {
  if (!method || method === "None") return null;
  if (typeof method !== "object") return null;
  const record = method as Record<string, { url?: string; address?: string }>;
  if (record.OneClick) return "One-click unsubscribe";
  if (record.Mailto) return `Sends an email to ${record.Mailto.address ?? "the list"}`;
  if (record.HttpLink || record.BodyLink) return "Opens the sender's unsubscribe page";
  return null;
}
