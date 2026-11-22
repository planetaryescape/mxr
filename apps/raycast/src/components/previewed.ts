import { Alert, Toast, confirmAlert, showToast } from "@raycast/api";
import { undoMutation } from "../bridge/modes";
import { keyFor } from "./keys";

export interface Previewed<R> {
  title: string;
  confirm: string;
  preview: () => Promise<R>;
  commit: () => Promise<R>;
  describe: (preview: R) => string;
  failures?: (result: R) => string[];
  undoId?: (result: R) => string | null | undefined;
  done: string;
  refresh: () => void;
}

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

export function itemErrors(items: { error?: string | null }[]): string[] {
  return items.flatMap((item) => (item.error ? [item.error] : []));
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
