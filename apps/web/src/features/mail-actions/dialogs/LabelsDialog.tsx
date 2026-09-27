import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Minus, Pencil, Plus, Tag, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
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
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { createLabel, deleteLabel, renameLabel, shellKey } from "@/features/mailbox/api";
import { lensesFromShell } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

import { performMailAction } from "../mailMutations";
import { describeTarget } from "../mailVerbs";
import type { MailTarget } from "../target";

type Presence = "all" | "some" | "none";

/**
 * Add and remove labels in one go, like Gmail's label menu: each label
 * shows whether all, some or none of the target has it; Enter
 * toggles; ⌘Enter applies. Typing a new name offers to create it.
 */
export function LabelsDialog({ target, onClose }: { target: MailTarget; onClose: () => void }) {
  const shell = useShellQuery();
  const qc = useQueryClient();
  const [query, setQuery] = useState("");
  const [staged, setStaged] = useState<Map<string, boolean>>(new Map());
  const [managing, setManaging] = useState<string | null>(null);

  const labels = useMemo(
    () =>
      lensesFromShell(shell.data)
        .filter((lens) => lens.section === "labels")
        .map((lens) => lens.label),
    [shell.data],
  );
  const presence = useMemo(() => {
    const map = new Map<string, Presence>();
    for (const name of labels) {
      const having = target.rows.filter((row) =>
        row.labels?.some((label) => label.name === name),
      ).length;
      map.set(name, having === 0 ? "none" : having === target.rows.length ? "all" : "some");
    }
    return map;
  }, [labels, target.rows]);

  const trimmed = query.trim();
  const exists = labels.some(
    (name) => name.localeCompare(trimmed, undefined, { sensitivity: "accent" }) === 0,
  );

  const effective = (name: string): Presence => {
    const change = staged.get(name);
    if (change === undefined) return presence.get(name) ?? "none";
    return change ? "all" : "none";
  };

  const toggle = (name: string) =>
    setStaged((previous) => {
      const next = new Map(previous);
      const current = effective(name);
      const wanted = current !== "all";
      const original = presence.get(name) ?? "none";
      if ((wanted && original === "all") || (!wanted && original === "none")) next.delete(name);
      else next.set(name, wanted);
      return next;
    });

  const create = useMutation({
    mutationFn: (name: string) => createLabel({ name, accountId: target.accountId }),
    onSuccess: (_, name) => {
      setStaged((previous) => new Map(previous).set(name, true));
      setQuery("");
      void qc.invalidateQueries({ queryKey: shellKey });
      toast.success(`Created ${name}`);
    },
    onError: (error) => toast.error("Couldn't create label", { description: error.message }),
  });

  const add = [...staged].filter(([, on]) => on).map(([name]) => name);
  const remove = [...staged].filter(([, on]) => !on).map(([name]) => name);
  const apply = () => {
    if (add.length === 0 && remove.length === 0) {
      onClose();
      return;
    }
    onClose();
    void performMailAction("labels", target.messageIds, { payload: { add, remove } });
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md gap-0 overflow-hidden p-0">
        <div className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle>Labels</DialogTitle>
          <DialogDescription className="mt-1 truncate">{describeTarget(target)}</DialogDescription>
        </div>
        <Command
          loop
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              apply();
            }
          }}
        >
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder="Find or create a label…"
          />
          <CommandList className="max-h-72">
            <CommandEmpty>
              {trimmed ? null : "No labels yet. Type a name to create one."}
            </CommandEmpty>
            <CommandGroup>
              {labels.map((name) => {
                const state = effective(name);
                const changed = staged.has(name);
                return (
                  <CommandItem
                    key={name}
                    value={name}
                    onSelect={() => toggle(name)}
                    className="group gap-2.5"
                  >
                    <span
                      aria-hidden
                      className={cn(
                        "grid size-4 place-items-center rounded-[4px] border",
                        state === "none"
                          ? "border-border-strong"
                          : "border-primary bg-primary text-primary-foreground",
                      )}
                    >
                      {state === "all" ? <Check className="size-3" strokeWidth={3} /> : null}
                      {state === "some" ? <Minus className="size-3" strokeWidth={3} /> : null}
                    </span>
                    <span className="min-w-0 flex-1 truncate">{name}</span>
                    {changed ? (
                      <span className="font-mono text-2xs text-primary">
                        {state === "all" ? "+ add" : "− remove"}
                      </span>
                    ) : null}
                    <button
                      type="button"
                      tabIndex={-1}
                      aria-label={`Manage ${name}`}
                      className="invisible rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground group-hover:visible"
                      onClick={(event) => {
                        event.stopPropagation();
                        setManaging(name);
                      }}
                    >
                      <Pencil className="size-3" />
                    </button>
                  </CommandItem>
                );
              })}
              {trimmed && !exists ? (
                <CommandItem
                  value={`create ${trimmed}`}
                  onSelect={() => create.mutate(trimmed)}
                  disabled={create.isPending}
                >
                  <Plus className="size-4" />
                  Create “{trimmed}”
                </CommandItem>
              ) : null}
            </CommandGroup>
          </CommandList>
        </Command>
        <div className="flex items-center justify-between gap-2 border-t border-border px-4 py-3">
          <span className="text-2xs text-muted-foreground">
            {add.length + remove.length > 0
              ? `${plural(add.length, "add")}, ${plural(remove.length, "removal")}`
              : "Enter toggles a label"}
          </span>
          <span className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button size="sm" onClick={apply}>
              <Tag className="size-3.5" /> Apply{" "}
              <KeyChip className="border-primary-foreground/30 bg-transparent text-primary-foreground/80">
                ⌘↵
              </KeyChip>
            </Button>
          </span>
        </div>
        {managing ? (
          <ManageLabel
            name={managing}
            accountId={target.accountId}
            onClose={() => setManaging(null)}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function ManageLabel({
  name,
  accountId,
  onClose,
}: {
  name: string;
  accountId?: string;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [draft, setDraft] = useState(name);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: shellKey });
    void qc.invalidateQueries({ queryKey: ["mailbox"] });
  };
  const rename = useMutation({
    mutationFn: () => renameLabel({ oldName: name, newName: draft.trim(), accountId }),
    onSuccess: () => {
      toast.success(`Renamed to ${draft.trim()}`);
      refresh();
      onClose();
    },
    onError: (error) => toast.error("Rename failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteLabel({ name, accountId }),
    onSuccess: () => {
      toast.success(`Deleted ${name}`);
      refresh();
      onClose();
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  return (
    <div className="border-t border-border bg-muted/40 px-4 py-3">
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (draft.trim() && draft.trim() !== name) rename.mutate();
        }}
      >
        <Input
          autoFocus
          aria-label={`Rename ${name}`}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => event.key === "Escape" && (event.stopPropagation(), onClose())}
          className="h-8"
        />
        <Button
          type="submit"
          size="sm"
          disabled={rename.isPending || !draft.trim() || draft.trim() === name}
        >
          Rename
        </Button>
        <Button
          type="button"
          size="icon-sm"
          variant="destructive"
          aria-label={`Delete ${name}`}
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </form>
      <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{name}”?</AlertDialogTitle>
            <AlertDialogDescription>
              Messages keep everything else; they just lose this label. This can't be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => remove.mutate()}>
              Delete label
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
