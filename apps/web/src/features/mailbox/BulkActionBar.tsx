import { Archive, CheckCheck, Clock, Mail, ShieldAlert, Star, Trash2, X } from "lucide-react";
import { useState } from "react";

import { SnoozeDialog } from "./SnoozeDialog";
import type { MailAction } from "./useOptimisticMailMutation";
import { useOptimisticMailMutation } from "./useOptimisticMailMutation";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useSelection } from "@/state/selectionStore";

const actions: Array<{ action: MailAction; label: string; icon: typeof Archive }> = [
  { action: "archive", label: "Archive", icon: Archive },
  { action: "trash", label: "Trash", icon: Trash2 },
  { action: "spam", label: "Spam", icon: ShieldAlert },
  { action: "star", label: "Star", icon: Star },
  { action: "read", label: "Read", icon: CheckCheck },
  { action: "unread", label: "Unread", icon: Mail },
];

const confirmBeforeBulk = new Set<MailAction>(["archive", "trash", "spam"]);

export function BulkActionBar() {
  const ids = useSelection((state) => state.ids);
  const clear = useSelection((state) => state.clear);
  const [snoozeOpen, setSnoozeOpen] = useState(false);
  const selected = [...ids];
  if (selected.length === 0) return null;
  return (
    <>
      <div className="absolute inset-x-4 bottom-4 z-10 flex items-center gap-2 rounded-xl border border-border-strong bg-popover/95 px-3 py-2 shadow-2xl backdrop-blur">
        <div className="mr-2 font-mono text-2xs text-muted-foreground">
          {selected.length} selected
        </div>
        {actions.map((item) => (
          <BulkButton
            key={item.action}
            action={item.action}
            label={item.label}
            icon={item.icon}
            ids={selected}
          />
        ))}
        <Button variant="secondary" size="sm" onClick={() => setSnoozeOpen(true)}>
          <Clock className="size-3" />
          Snooze
        </Button>
        <Button variant="ghost" size="sm" className="ml-auto" onClick={clear}>
          <X className="size-3" />
          Clear
        </Button>
      </div>
      <SnoozeDialog
        open={snoozeOpen}
        messageIds={selected}
        onOpenChange={setSnoozeOpen}
        onSnoozed={clear}
      />
    </>
  );
}

function BulkButton({
  action,
  label,
  icon: Icon,
  ids,
}: {
  action: MailAction;
  label: string;
  icon: typeof Archive;
  ids: string[];
}) {
  const mutation = useOptimisticMailMutation(action);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const needsConfirm = confirmBeforeBulk.has(action);

  function run() {
    mutation.mutate(ids);
  }

  return (
    <>
      <Button
        variant="secondary"
        size="sm"
        onClick={() => (needsConfirm ? setConfirmOpen(true) : run())}
        disabled={mutation.isPending}
      >
        <Icon className="size-3" />
        {label}
      </Button>
      {needsConfirm ? (
        <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>
                {label} {ids.length} {ids.length === 1 ? "message" : "messages"}?
              </DialogTitle>
              <DialogDescription>
                This will apply to every selected message. Use Undo from the success toast if you
                change your mind.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="ghost" onClick={() => setConfirmOpen(false)}>
                Cancel
              </Button>
              <Button
                variant={action === "trash" || action === "spam" ? "destructive" : "default"}
                onClick={() => {
                  setConfirmOpen(false);
                  run();
                }}
              >
                <Icon className="size-3" />
                Confirm {label}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      ) : null}
    </>
  );
}
