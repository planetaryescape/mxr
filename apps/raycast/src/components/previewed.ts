import { Alert, Toast, confirmAlert, showToast } from "@raycast/api";
import { undoMutation } from "../bridge/modes";
import { keyFor } from "./keys";

export interface Previewed<R> {
  /** Confirm-dialog title, e.g. "Done here?" */
  title: string;
  /** Button that commits. */
  confirm: string;
  /** The dry run: the same selection the real change will act on. */
  preview: () => Promise<R>;
  commit: () => Promise<R>;
  /** What the daemon says the change will do, for the dialog. */
  describe: (preview: R) => string;
  /** Items the daemon reports it could not change (HTTP 200 is not every
   * item succeeding). */
  failures?: (result: R) => string[];
  /** The undo handle of a committed change, when the daemon kept one. */
  undoId?: (result: R) => string | null | undefined;
  /** Toast title once it is done. */
  done: string;
  /** Refresh the list after the change, or after an undo. */
  refresh: () => void;
}

/** Preview a change, ask, commit, then offer Undo: the daemon's preview path. */
export async function runPreviewed<R>(change: Previewed<R>): Promise<void> {
  const toast = await showToast({ style: Toast.Style.Animated, title: "Checking…" });
  try {
    const preview = await change.preview();
    await toast.hide();
    const blocked = change.failures?.(preview) ?? [];
    const confirmed = await confirmAlert({
      title: change.title,
      message: [
        change.describe(preview),
        ...(blocked.length ? ["Won't change:", ...blocked.map((reason) => `• ${reason}`)] : []),
      ].join("\n"),
      primaryAction: { title: change.confirm, style: Alert.ActionStyle.Default },
    });
    if (!confirmed) return;

    const progress = await showToast({ style: Toast.Style.Animated, title: "Working…" });
    const result = await change.commit();
    change.refresh();
    const undoId = change.undoId?.(result);
    const failed = change.failures?.(result) ?? [];
    progress.style = failed.length ? Toast.Style.Failure : Toast.Style.Success;
    progress.title = failed.length ? `${change.done}, not everything` : change.done;
    if (failed.length) progress.message = failed.join(" ");
    if (undoId) {
      progress.primaryAction = {
        title: "Undo",
        shortcut: keyFor("mail.undo"),
        onAction: async (shown) => {
          shown.style = Toast.Style.Animated;
          shown.title = "Undoing…";
          try {
            await undoMutation(undoId);
            change.refresh();
            shown.style = Toast.Style.Success;
            shown.title = "Undone";
          } catch (error) {
            shown.style = Toast.Style.Failure;
            shown.title = "Undo failed";
            shown.message = message(error);
          }
        },
      };
    }
  } catch (error) {
    await showToast({ style: Toast.Style.Failure, title: "mxr", message: message(error) });
  }
}

/** Per-item errors from the daemon's outcome lists. */
export function itemErrors(items: { error?: string | null }[]): string[] {
  return items.flatMap((item) => (item.error ? [item.error] : []));
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
