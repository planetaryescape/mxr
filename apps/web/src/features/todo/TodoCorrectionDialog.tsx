import { useEffect, useRef, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { KeyChip } from "@/components/KeyChip";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import type { Todo } from "./api";
import { correctRow } from "./todoVerbs";
import { optionLetter, pressedOverlayKey } from "@/lib/keys/overlayKeys";

/** Each reason, its label and its key's id in the key definition. */
const REASONS = [
  ["not_a_todo", "Not a to-do", "correct-todo.not-a-todo"],
  ["not_mine", "Not mine", "correct-todo.not-mine"],
  ["already_done", "Already done", "correct-todo.already-done"],
  ["wrong_details", "Wrong details", "correct-todo.wrong-details"],
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
        {/* A picker, as in the TUI (blueprint 35): j and k move, Enter runs
            the focused reason, its digit runs it at once. */}
        <div
          className="grid gap-1"
          onKeyDown={(event) => {
            if (event.metaKey || event.ctrlKey || event.altKey) return;
            const picked = REASONS.find(([, , id]) => pressedOverlayKey(event, id));
            if (picked) {
              event.preventDefault();
              void choose(picked[0]);
              return;
            }
            const step = pressedOverlayKey(event, "overlay.down")
              ? 1
              : pressedOverlayKey(event, "overlay.up")
                ? -1
                : 0;
            if (step === 0) return;
            event.preventDefault();
            const buttons = [...event.currentTarget.querySelectorAll("button")];
            const at = buttons.findIndex((button) => button === document.activeElement);
            buttons[Math.max(0, Math.min(buttons.length - 1, at + step))]?.focus();
          }}
        >
          {REASONS.map(([reason, label, id], index) => (
            <Button
              key={reason}
              autoFocus={index === 0}
              variant="ghost"
              disabled={saving}
              onClick={() => void choose(reason)}
              aria-keyshortcuts={optionLetter(id)}
              className="justify-between"
            >
              {label}
              <span aria-hidden="true">
                <KeyChip>{optionLetter(id)}</KeyChip>
              </span>
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
