import { create } from "zustand";

import type { MoveSubject } from "@/features/arrivals/moves";
import type { RecordData, RecordFilter } from "@/features/archive/api";
import type { MailKind, SweepScope } from "@/features/places/api";
import type { Todo } from "@/features/todo/api";

import type { NotNowPicker } from "./notNow";
import type { MailAction } from "./pendingMailOps";
import type { MailTarget } from "./target";

export type MailDialog =
  | { kind: "snooze"; target: MailTarget; onDone?: () => void; onLater?: () => void }
  | { kind: "not-now"; picker: NotNowPicker }
  | {
      kind: "reply-later";
      target: MailTarget;
      waiting: boolean;
    }
  | { kind: "labels"; target: MailTarget }
  | { kind: "categories"; target: MailTarget }
  | {
      kind: "move";
      target: MailTarget;
      route?: { fromQueueLabel: string };
      onDone?: () => void;
    }
  | { kind: "unsubscribe"; target: MailTarget; onDone?: () => void }
  | { kind: "links"; target: MailTarget }
  | {
      kind: "sweep";
      scope: SweepScope;
      senderLabel?: string;
      shownHere?: number;
    }
  | {
      kind: "sender-kind";
      accountId: string;
      senderEmail: string;
      senderLabel: string;
      current?: MailKind;
    }
  | { kind: "move-to-mode"; subject: MoveSubject; sender: boolean }
  | { kind: "todo-schedule"; todo: Todo }
  | { kind: "todo-edit"; todo: Todo }
  | { kind: "todo-correct"; todo: Todo }
  | { kind: "todo-make"; messageId: string; suggestion: string; subject?: string }
  | { kind: "record-edit"; record: RecordData }
  | { kind: "record-export"; account: string | null; filter: RecordFilter }
  | { kind: "pass-to-mode"; messageId: string; subject?: string }
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
