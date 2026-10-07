/*
 * Sweep: archive everything unpinned in a place or one sender's bundle.
 * The dialog shows the daemon's dry run; confirming hands back its preview
 * token, so the daemon archives only the messages that preview listed (and
 * that are still here and unpinned) as an undoable job.
 */

import { toast } from "sonner";

import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { awaitJob } from "@/features/mail-actions/mailMutationJobs";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo, offerUndo, undoAll } from "@/features/mail-actions/mailUndo";
import { soundFor } from "@/features/mail-actions/verbFeedback";
import { playSound } from "@/features/sound/player";
import { plural } from "@/lib/format";

import { commitSweep, type SweepPreview, type SweepScope } from "./api";
import { isStalePreview, PLACE_TITLES } from "./placeCopy";

export async function runSweep(
  scope: SweepScope,
  preview: SweepPreview,
  senderLabel?: string,
): Promise<boolean> {
  const where = senderLabel ?? PLACE_TITLES[scope.place];
  const claim = claimUndo();
  try {
    const token = preview.preview_token;
    if (!token) {
      claim.settle(null);
      toast.info(`Nothing to sweep from ${where}`);
      return true;
    }
    const swept = await commitSweep(scope, token);
    if (!swept.job) {
      claim.settle(null);
      toast.info(`Nothing left to sweep from ${where}`);
      return true;
    }
    const response = await awaitJob(swept.job.job_id, {
      start: `Archiving ${plural(swept.preview.count, "message")} from ${where}…`,
      progress: (completed, total) =>
        `Archiving ${completed.toLocaleString()} of ${plural(total, "message")}`,
    });
    const archived = response.result?.succeeded ?? 0;
    const undoIds = response.result?.undo_ids ?? [];
    if (!response.ok) {
      // What did land stays undoable: its chunks' undo ids came back too.
      claim.settle(undoIds.length > 0 ? () => undoAll(undoIds) : null);
      const error = response.result?.accounts?.[0]?.error ?? "The daemon stopped the sweep";
      toast.error(`Swept ${plural(archived, "message")} before it stopped`, {
        description: error,
        ...(undoIds.length > 0
          ? { action: { label: "Undo", onClick: () => void undoAll(undoIds) } }
          : {}),
      });
      return false;
    }
    // Pinned (or moved away) after the preview: the daemon leaves those.
    const left = response.result?.skipped ?? 0;
    const leftNote =
      left > 0 ? `; ${plural(left, "message")} changed since the preview and stayed` : "";
    const sound = soundFor("sweep");
    if (archived > 0 && sound) playSound(sound);
    claim.settle(
      offerUndo(
        "sweep",
        `Archived ${plural(archived, "message")} from ${where}${leftNote}`,
        `sweep-${swept.job.job_id}`,
        undoIds.length > 0 ? () => undoAll(undoIds) : null,
        claim.run,
      ),
    );
    return true;
  } catch (caught) {
    claim.settle(null);
    if (isStalePreview(caught)) {
      // A preview is good once, for a few minutes: nothing was archived.
      toast.error("That preview has expired", {
        description: "Nothing was archived. Preview again to see what the sweep covers now.",
        action: {
          label: "Preview again",
          onClick: () => openMailDialog({ kind: "sweep", scope, senderLabel }),
        },
      });
      return false;
    }
    toast.error(`Couldn't sweep ${where}`, {
      description: caught instanceof Error ? caught.message : String(caught),
    });
    return false;
  } finally {
    await invalidateMailQueries().catch(() => undefined);
  }
}
