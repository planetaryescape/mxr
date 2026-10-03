import { SenderKindDialog } from "@/features/places/SenderKindDialog";
import { SweepDialog } from "@/features/places/SweepDialog";
import {
  LetGoAllDialog,
  MakeTodoDialog,
  TodoEditDialog,
  TodoScheduleDialog,
} from "@/features/todo/TodoDialogs";

import { ConfirmDialog } from "./dialogs/ConfirmDialog";
import { LabelsDialog } from "./dialogs/LabelsDialog";
import { LinksDialog } from "./dialogs/LinksDialog";
import { MoveDialog } from "./dialogs/MoveDialog";
import { ReplyLaterDialog } from "./dialogs/ReplyLaterDialog";
import { SnoozeDialog } from "./dialogs/SnoozeDialog";
import { UnsubscribeDialog } from "./dialogs/UnsubscribeDialog";
import { useMailDialogs } from "./mailDialogStore";
import { describeTarget } from "./mailVerbs";

/** Mounted once in the shell; renders whichever mail dialog is open. */
export function MailDialogs() {
  const dialog = useMailDialogs((s) => s.dialog);
  const close = useMailDialogs((s) => s.close);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "snooze":
      return (
        <SnoozeDialog
          open
          messageIds={dialog.target.messageIds}
          subject={describeTarget(dialog.target)}
          onOpenChange={(open) => !open && close()}
          onSnoozed={dialog.onDone}
        />
      );
    case "reply-later":
      return (
        <ReplyLaterDialog
          target={dialog.target}
          subject={describeTarget(dialog.target)}
          waiting={dialog.waiting}
          onClose={close}
        />
      );
    case "labels":
      return <LabelsDialog target={dialog.target} onClose={close} />;
    case "move":
      return (
        <MoveDialog
          target={dialog.target}
          route={dialog.route}
          onDone={dialog.onDone}
          onClose={close}
        />
      );
    case "unsubscribe":
      return <UnsubscribeDialog target={dialog.target} onDone={dialog.onDone} onClose={close} />;
    case "links":
      return <LinksDialog target={dialog.target} onClose={close} />;
    case "sweep":
      return (
        <SweepDialog
          scope={dialog.scope}
          senderLabel={dialog.senderLabel}
          shownHere={dialog.shownHere}
          onClose={close}
        />
      );
    case "sender-kind":
      return (
        <SenderKindDialog
          accountId={dialog.accountId}
          senderEmail={dialog.senderEmail}
          senderLabel={dialog.senderLabel}
          current={dialog.current}
          onClose={close}
        />
      );
    case "todo-schedule":
      return <TodoScheduleDialog todo={dialog.todo} onClose={close} />;
    case "todo-edit":
      return <TodoEditDialog todo={dialog.todo} onClose={close} />;
    case "todo-make":
      return (
        <MakeTodoDialog
          messageId={dialog.messageId}
          suggestion={dialog.suggestion}
          subject={dialog.subject}
          onClose={close}
        />
      );
    case "todo-let-go-all":
      return <LetGoAllDialog account={dialog.account} onClose={close} />;
    case "confirm":
      return (
        <ConfirmDialog
          target={dialog.target}
          action={dialog.action}
          onConfirm={dialog.onConfirm}
          onClose={close}
        />
      );
  }
}
