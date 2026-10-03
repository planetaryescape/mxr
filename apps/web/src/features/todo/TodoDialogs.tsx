import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useState, type FormEvent } from "react";

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
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { describeChoice, type TimeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime } from "@/features/time/useNaturalTime";
import { plural } from "@/lib/format";

import type { Todo } from "./api";
import {
  currentValue,
  editRow,
  letGoAll,
  makeTodo,
  previewLetGoAll,
  scheduleRow,
} from "./todoVerbs";

/**
 * `Z`: show the row on your own day. The same natural-time field snooze
 * uses: the daemon reads the words and the dialog shows the exact time
 * before anything is stored. The due date stays as it is.
 */
export function TodoScheduleDialog({ todo, onClose }: { todo: Todo; onClose: () => void }) {
  const time = useNaturalTime({ enabled: true });
  async function commit(choice: TimeChoice) {
    onClose();
    time.reset();
    await scheduleRow(todo, choice.at, describeChoice(choice));
  }
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md grid-cols-[minmax(0,1fr)]">
        <DialogHeader>
          <DialogTitle>Schedule</DialogTitle>
          <DialogDescription className="truncate">
            {todo.title} shows up then. {todo.due_at ? "The due date stays as it is." : ""}
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void time.commit(commit);
          }}
        >
          <NaturalTimeInput
            id="todo-schedule"
            state={time}
            onCommit={commit}
            autoFocus
            placeholder="mon 9am, fri, in 3d"
            className="min-w-0 flex-1"
          />
          <Button type="submit" size="sm" disabled={!time.canCommit}>
            Schedule
          </Button>
        </form>
        <DialogFooter className="text-2xs text-muted-foreground sm:justify-start">
          Undo with u for about a minute afterwards.
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const EDIT_FIELDS = [
  { field: "title", label: "What to do", hint: "Starts with the verb: Pay council tax" },
  { field: "counterparty", label: "Who", hint: "Camden Council" },
  { field: "amount", label: "Amount", hint: "£142.00" },
  { field: "due", label: "Due", hint: "fri 9 oct, in 2w; empty clears it" },
] as const;

/** The due date as a phrase the daemon reads back: "9 Oct 2026". */
function dueValue(todo: Todo): string {
  if (!todo.due_at) return "";
  const date = new Date(todo.due_at);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** `,`: correct the fields mxr read. Only changed fields are sent. */
export function TodoEditDialog({ todo, onClose }: { todo: Todo; onClose: () => void }) {
  const initial: Record<(typeof EDIT_FIELDS)[number]["field"], string> = {
    title: currentValue(todo, "title"),
    counterparty: currentValue(todo, "counterparty"),
    amount: currentValue(todo, "amount"),
    due: dueValue(todo),
  };
  const [values, setValues] = useState(initial);
  const changed = EDIT_FIELDS.filter(({ field }) => values[field].trim() !== initial[field].trim());
  function submit(event: FormEvent) {
    event.preventDefault();
    if (changed.length === 0 || !values.title.trim()) return;
    onClose();
    void editRow(
      todo,
      changed.map(({ field }) => ({ field, value: values[field].trim() })),
    );
  }
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Edit to-do</DialogTitle>
          <DialogDescription>
            What you change here is yours: mxr won't rewrite it.
          </DialogDescription>
        </DialogHeader>
        <form className="grid gap-3" onSubmit={submit}>
          {EDIT_FIELDS.map(({ field, label, hint }, index) => (
            <label key={field} className="grid gap-1 text-[13px]">
              <span className="font-medium">{label}</span>
              <Input
                autoFocus={index === 0}
                value={values[field]}
                placeholder={hint}
                onChange={(event) => setValues({ ...values, [field]: event.target.value })}
              />
            </label>
          ))}
          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={changed.length === 0 || !values.title.trim()}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** `t` on a conversation: what to do about it, and when, in your words. */
export function MakeTodoDialog({
  messageId,
  suggestion,
  subject,
  onClose,
}: {
  messageId: string;
  suggestion: string;
  subject?: string;
  onClose: () => void;
}) {
  const [title, setTitle] = useState(suggestion);
  const [due, setDue] = useState("");
  function submit(event: FormEvent) {
    event.preventDefault();
    if (!title.trim()) return;
    onClose();
    void makeTodo({ messageId, title: title.trim(), due: due.trim() || undefined });
  }
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md grid-cols-[minmax(0,1fr)]">
        <DialogHeader>
          <DialogTitle>Make a to-do</DialogTitle>
          <DialogDescription className="truncate">
            {subject ? `From "${subject}". ` : ""}Write what to do, starting with the verb.
          </DialogDescription>
        </DialogHeader>
        <form className="grid gap-3" onSubmit={submit}>
          <label className="grid gap-1 text-[13px]">
            <span className="font-medium">What to do</span>
            <Input autoFocus value={title} onChange={(event) => setTitle(event.target.value)} />
          </label>
          <label className="grid gap-1 text-[13px]">
            <span className="font-medium">Due (optional)</span>
            <Input
              value={due}
              placeholder="fri, 9 oct, in 2w"
              onChange={(event) => setDue(event.target.value)}
            />
          </label>
          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={!title.trim()}>
              Add to To do
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * "Let go of all" in the catch-up. The list is the daemon's dry run, and
 * the confirm button lets go of exactly those rows.
 */
export function LetGoAllDialog({
  account,
  onClose,
}: {
  account: string | null;
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: ["todo-let-go-all-preview", account ?? "all"],
    queryFn: () => previewLetGoAll(account),
    staleTime: 0,
    gcTime: 0,
  });
  const rows = preview.data?.changed ?? [];
  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent data-testid="let-go-all-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {preview.data
              ? rows.length > 0
                ? `Let go of ${plural(rows.length, "to-do")}?`
                : "Nothing left to let go of"
              : "Counting what to let go of…"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {preview.isError
              ? `Preview failed: ${preview.error.message}`
              : "They move to the Expired list, one key from restore. Undo with u."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : rows.length > 0 ? (
          <ul
            aria-label="To-dos to let go of"
            className="grid max-h-64 gap-0.5 overflow-y-auto border-l border-border pl-3 text-[13px] text-muted-foreground"
          >
            {rows.map((todo) => (
              <li
                key={todo.id}
                data-testid="let-go-preview-row"
                data-id={todo.id}
                className="truncate"
              >
                {todo.title}
              </li>
            ))}
          </ul>
        ) : null}
        <AlertDialogFooter>
          {/* Opens on Cancel: a slip to A then Enter must not let go of everything. */}
          <AlertDialogCancel autoFocus>{rows.length === 0 ? "Close" : "Cancel"}</AlertDialogCancel>
          {rows.length > 0 ? (
            <AlertDialogAction
              onClick={() => {
                onClose();
                void letGoAll(account, rows);
              }}
            >
              Let go of {plural(rows.length, "to-do")}
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
