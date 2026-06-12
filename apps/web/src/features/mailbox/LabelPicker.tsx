import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Pencil, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { deleteLabel, fetchShell, renameLabel, shellKey } from "@/features/mailbox/api";
import {
  useOptimisticMailMutation,
  type MailAction,
} from "@/features/mailbox/useOptimisticMailMutation";
import type { SidebarItem } from "@/features/mailbox/types";

interface LabelPickerProps {
  /** "label-add" / "label-remove" — adds or removes the chosen label. */
  mode: Extract<MailAction, "label-add" | "label-remove">;
  messageIds: string[];
  /** Pre-applied label names (for "remove" mode UX). */
  appliedLabels?: string[];
  onClose: () => void;
}

export function LabelPicker({ mode, messageIds, appliedLabels, onClose }: LabelPickerProps) {
  const shell = useQuery({ queryKey: shellKey, queryFn: fetchShell, staleTime: 60_000 });
  const [filter, setFilter] = useState("");

  const labelItems = useMemo(() => {
    const sections = shell.data?.sidebar?.sections ?? [];
    const labels: SidebarItem[] = [];
    for (const section of sections) {
      for (const item of section.items) {
        if (item.lens?.kind === "label") labels.push(item);
      }
    }
    if (mode === "label-remove" && appliedLabels?.length) {
      return labels.filter((l) => appliedLabels.includes(l.label));
    }
    if (filter) {
      const q = filter.toLowerCase();
      return labels.filter((l) => l.label.toLowerCase().includes(q));
    }
    return labels;
  }, [shell.data?.sidebar?.sections, filter, mode, appliedLabels]);

  return (
    <div className="space-y-3">
      <div>
        <h3 className="text-sm font-semibold text-foreground">
          {mode === "label-add" ? "Apply label" : "Remove label"}
        </h3>
        <p className="text-2xs text-muted-foreground">
          {messageIds.length} message{messageIds.length === 1 ? "" : "s"}
        </p>
      </div>
      <Input
        autoFocus
        aria-label="Filter labels"
        placeholder="Filter labels"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <div className="space-y-1">
        {labelItems.length === 0 ? (
          <div className="text-xs text-muted-foreground">No labels.</div>
        ) : (
          labelItems.map((label) => (
            <LabelRow
              key={label.id}
              label={label.label}
              mode={mode}
              messageIds={messageIds}
              onApplied={onClose}
            />
          ))
        )}
      </div>
    </div>
  );
}

function LabelRow({
  label,
  mode,
  messageIds,
  onApplied,
}: {
  label: string;
  mode: Extract<MailAction, "label-add" | "label-remove">;
  messageIds: string[];
  onApplied: () => void;
}) {
  const qc = useQueryClient();
  const mutation = useOptimisticMailMutation(mode, { payload: { label } });
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(label);

  const invalidateLabels = () => {
    void qc.invalidateQueries({ queryKey: shellKey });
  };

  const rename = useMutation({
    mutationFn: () => renameLabel({ oldName: label, newName: draft.trim() }),
    onSuccess: () => {
      toast.success(`Renamed to ${draft.trim()}`);
      setEditing(false);
      invalidateLabels();
    },
    onError: (error) => toast.error("Rename failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteLabel({ name: label }),
    onSuccess: () => {
      toast.success(`Deleted ${label}`);
      invalidateLabels();
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  if (editing) {
    return (
      <form
        className="flex items-center gap-1"
        onSubmit={(event) => {
          event.preventDefault();
          if (draft.trim() && draft.trim() !== label) rename.mutate();
          else setEditing(false);
        }}
      >
        <Input
          autoFocus
          aria-label={`Rename label ${label}`}
          className="h-8 text-xs"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setDraft(label);
              setEditing(false);
            }
          }}
        />
        <Button
          type="submit"
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          aria-label="Save label name"
          disabled={rename.isPending}
        >
          <Check className="size-3" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          aria-label="Cancel rename"
          onClick={() => {
            setDraft(label);
            setEditing(false);
          }}
        >
          <X className="size-3" />
        </Button>
      </form>
    );
  }

  return (
    <div className="group flex items-center gap-1">
      <Button
        variant="ghost"
        className="h-8 flex-1 justify-start text-xs"
        disabled={mutation.isPending}
        onClick={() => {
          mutation.mutate(messageIds, {
            onSuccess: () => onApplied(),
          });
        }}
      >
        {label}
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="size-8 shrink-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        aria-label={`Rename label ${label}`}
        onClick={() => {
          setDraft(label);
          setEditing(true);
        }}
      >
        <Pencil className="size-3" />
      </Button>
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-8 shrink-0 text-destructive opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            aria-label={`Delete label ${label}`}
            disabled={remove.isPending}
          >
            <Trash2 className="size-3" />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete label “{label}”?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes the label from mxr. Messages carrying it stay, but lose this label.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={remove.isPending}
              onClick={() => remove.mutate()}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
