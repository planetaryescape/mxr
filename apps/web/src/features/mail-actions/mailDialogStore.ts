/*
 * Which mail dialog is open and for what target. One host (MailDialogs)
 * renders them, so the list, the reader and the palette open identical
 * dialogs instead of each owning a copy.
 */

import { create } from "zustand";

import type { MoveSubject } from "@/features/arrivals/moves";
import type { RecordData, RecordFilter } from "@/features/archive/api";
import type { MailKind, SweepScope } from "@/features/places/api";
import type { Todo } from "@/features/todo/api";

import type { MailAction } from "./pendingMailOps";
import type { MailTarget } from "./target";

export type MailDialog =
  | { kind: "snooze"; target: MailTarget; onDone?: () => void }
  | {
      /** Reply later at a time (`b`); on Waiting on, back if nobody replies. */
      kind: "reply-later";
      target: MailTarget;
      waiting: boolean;
    }
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
      /** Preview a sweep from the daemon's dry run, then archive. */
      kind: "sweep";
      scope: SweepScope;
      /** Names the bundle in the title when sweeping one sender. */
      senderLabel?: string;
      /** Of what the sweep covers, how many messages are on screen. */
      shownHere?: number;
    }
  | {
      /** Move a sender to a kind, remembered for their future mail. */
      kind: "sender-kind";
      accountId: string;
      senderEmail: string;
      senderLabel: string;
      current?: MailKind;
    }
  /** Move one email (`X`) or its sender's mail (`K`) to a mode. */
  | { kind: "move-to-mode"; subject: MoveSubject; sender: boolean }
  /** To do: show a row on your own day (`Z`). */
  | { kind: "todo-schedule"; todo: Todo }
  /** To do: correct a row's fields (`,`). */
  | { kind: "todo-edit"; todo: Todo }
  /** Make a to-do from a conversation (`t`). */
  | { kind: "todo-make"; messageId: string; suggestion: string; subject?: string }
  /** Archive: fix or confirm a record's fields (`,`). */
  | { kind: "record-edit"; record: RecordData }
  /** Archive: preview the export of the records in view, then download it (`E`). */
  | { kind: "record-export"; account: string | null; filter: RecordFilter }
  /** Pass a conversation to another mode (`T`): this phase, file it in Archive. */
  | { kind: "pass-to-mode"; messageId: string; subject?: string }
  /** To do's catch-up: preview letting go of every row, then do exactly that. */
  | { kind: "todo-let-go-all"; account: string | null }
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
