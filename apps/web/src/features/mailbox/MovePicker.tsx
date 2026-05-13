import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { fetchShell } from "@/features/mailbox/api";
import { useOptimisticMailMutation } from "@/features/mailbox/useOptimisticMailMutation";
import type { SidebarItem } from "@/features/mailbox/types";

interface MovePickerProps {
  messageIds: string[];
  onClose: () => void;
}

export function MovePicker({ messageIds, onClose }: MovePickerProps) {
  const shell = useQuery({ queryKey: ["shell"], queryFn: fetchShell, staleTime: 60_000 });
  const [filter, setFilter] = useState("");

  const targets = useMemo(() => {
    const sections = shell.data?.sidebar?.sections ?? [];
    const items: SidebarItem[] = [];
    for (const section of sections) {
      for (const item of section.items) {
        if (item.lens?.kind === "label") items.push(item);
      }
    }
    if (filter) {
      const q = filter.toLowerCase();
      return items.filter((l) => l.label.toLowerCase().includes(q));
    }
    return items;
  }, [shell.data?.sidebar?.sections, filter]);

  return (
    <div className="space-y-3">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Move to label</h3>
        <p className="text-2xs text-muted-foreground">
          {messageIds.length} message{messageIds.length === 1 ? "" : "s"}
        </p>
      </div>
      <Input
        autoFocus
        aria-label="Filter destinations"
        placeholder="Filter destinations"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <div className="space-y-1">
        {targets.length === 0 ? (
          <div className="text-xs text-muted-foreground">No labels.</div>
        ) : (
          targets.map((label) => (
            <MoveRow
              key={label.id}
              label={label.label}
              messageIds={messageIds}
              onMoved={onClose}
            />
          ))
        )}
      </div>
    </div>
  );
}

function MoveRow({
  label,
  messageIds,
  onMoved,
}: {
  label: string;
  messageIds: string[];
  onMoved: () => void;
}) {
  const mutation = useOptimisticMailMutation("move", { payload: { label } });
  return (
    <Button
      variant="ghost"
      className="h-8 w-full justify-start text-xs"
      disabled={mutation.isPending}
      onClick={() => {
        mutation.mutate(messageIds, {
          onSuccess: () => onMoved(),
        });
      }}
    >
      {label}
    </Button>
  );
}
