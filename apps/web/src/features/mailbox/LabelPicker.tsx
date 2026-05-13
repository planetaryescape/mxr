import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { fetchShell } from "@/features/mailbox/api";
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
  const shell = useQuery({ queryKey: ["shell"], queryFn: fetchShell, staleTime: 60_000 });
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
  const mutation = useOptimisticMailMutation(mode, { payload: { label } });
  return (
    <Button
      variant="ghost"
      className="h-8 w-full justify-start text-xs"
      disabled={mutation.isPending}
      onClick={() => {
        mutation.mutate(messageIds, {
          onSuccess: () => onApplied(),
        });
      }}
    >
      {label}
    </Button>
  );
}
