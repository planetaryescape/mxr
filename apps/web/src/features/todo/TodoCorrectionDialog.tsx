import { useEffect, useRef, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import type { Todo } from "./api";
import { correctRow } from "./todoVerbs";

const REASONS = [
  ["not_a_todo", "Not a to-do"],
  ["not_mine", "Not mine"],
  ["already_done", "Already done"],
  ["wrong_details", "Wrong details"],
] as const;

export function TodoCorrectionDialog({ todo, onClose }: { todo: Todo; onClose: () => void }) {
  const [saving, setSaving] = useState(false);
  const active = useRef(true);
  const inFlight = useRef(false);
  const currentId = useRef(todo.id);
  useEffect(() => {
    currentId.current = todo.id;
  }, [todo.id]);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  async function choose(reason: (typeof REASONS)[number][0]) {
    if (reason === "wrong_details") {
      openMailDialog({ kind: "todo-edit", todo });
      return;
    }
    if (inFlight.current) return;
    inFlight.current = true;
    setSaving(true);
    const requestedId = todo.id;
    try {
      const saved = await correctRow(todo, reason);
      if (saved && active.current && currentId.current === requestedId) onClose();
    } finally {
      inFlight.current = false;
      if (active.current) setSaving(false);
    }
  }
  return (
    <Dialog open onOpenChange={(open) => !open && !saving && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Correct this to-do</DialogTitle>
          <DialogDescription>{todo.title}</DialogDescription>
        </DialogHeader>
        <blockquote className="border-l border-rule pl-3 text-text-2">{todo.why}</blockquote>
        <div className="grid gap-1">
          {REASONS.map(([reason, label], index) => (
            <Button
              key={reason}
              autoFocus={index === 0}
              variant="ghost"
              disabled={saving}
              onClick={() => void choose(reason)}
            >
              {label}
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
