/*
 * Which mail dialog is open and for what target. One host (MailDialogs)
 * renders them, so the list, the reader and the palette open identical
 * dialogs instead of each owning a copy.
 */

import { create } from "zustand";

import type { MailAction } from "./pendingMailOps";
import type { MailTarget } from "./target";

export type MailDialog =
  | { kind: "snooze"; target: MailTarget; onDone?: () => void }
  | { kind: "labels"; target: MailTarget }
  | {
      kind: "move";
      target: MailTarget;
      /** Set to route out of a queue label instead of a plain move. */
      route?: { fromQueueLabel: string };
      onDone?: () => void;
    }
  | { kind: "unsubscribe"; target: MailTarget; onDone?: () => void }
  | { kind: "links"; target: MailTarget }
  | {
      kind: "confirm";
      target: MailTarget;
      action: MailAction;
      onConfirm: () => void;
    };

interface MailDialogState {
  dialog: MailDialog | null;
  open: (dialog: MailDialog) => void;
  close: () => void;
}

export const useMailDialogs = create<MailDialogState>((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
  close: () => set({ dialog: null }),
}));

export function openMailDialog(dialog: MailDialog): void {
  useMailDialogs.getState().open(dialog);
}
