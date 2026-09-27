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
import { plural } from "@/lib/format";

import type { MailAction } from "../pendingMailOps";
import type { MailTarget } from "../target";

const VERB: Partial<Record<MailAction, string>> = {
  archive: "Archive",
  "read-and-archive": "Read and archive",
  trash: "Move to Trash",
  spam: "Mark as spam",
  move: "Move",
  snooze: "Snooze",
};

/**
 * Preview before a large or destructive batch. It lists the very rows the
 * action will receive, so what you confirm is what runs.
 */
export function ConfirmDialog({
  target,
  action,
  onConfirm,
  onClose,
}: {
  target: MailTarget;
  action: MailAction;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const verb = VERB[action] ?? "Apply to";
  const destructive = action === "trash" || action === "spam";
  const preview = target.rows.slice(0, 6);
  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {verb} {plural(target.conversations, "conversation")}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            {plural(target.messageIds.length, "message")} will change. You can undo for about a
            minute afterwards.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <ul className="max-h-48 overflow-auto rounded-md border border-border text-[13px]">
          {preview.map((row) => (
            <li
              key={row.id}
              className="flex gap-2 border-b border-border/60 px-3 py-1.5 last:border-0"
            >
              <span className="w-32 shrink-0 truncate text-muted-foreground">{row.sender}</span>
              <span className="min-w-0 truncate">{row.subject || "(no subject)"}</span>
            </li>
          ))}
          {target.rows.length > preview.length ? (
            <li className="px-3 py-1.5 text-muted-foreground">
              and {plural(target.rows.length - preview.length, "more conversation")}
            </li>
          ) : null}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            autoFocus
            variant={destructive ? "destructive" : "default"}
            onClick={() => {
              onClose();
              onConfirm();
            }}
          >
            {verb}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
