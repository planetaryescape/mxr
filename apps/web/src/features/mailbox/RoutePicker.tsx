import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { fetchShell } from "@/features/mailbox/api";
import type { SidebarItem } from "@/features/mailbox/types";
import { useOptimisticMailMutation } from "@/features/mailbox/useOptimisticMailMutation";

interface RoutePickerProps {
  messageIds: string[];
  fromQueueLabel: string;
  archive?: boolean;
  onClose: () => void;
}

export function RoutePicker({ messageIds, fromQueueLabel, archive = true, onClose }: RoutePickerProps) {
  const shell = useQuery({ queryKey: ["shell"], queryFn: fetchShell, staleTime: 60_000 });
  const [filter, setFilter] = useState("");

  const targets = useMemo(() => {
    const sections = shell.data?.sidebar?.sections ?? [];
    const items: SidebarItem[] = [];
    for (const section of sections) {
      for (const item of section.items) {
        if (item.lens?.kind === "label" && item.label !== fromQueueLabel) items.push(item);
      }
    }
    if (!filter) return items;
    const q = filter.toLowerCase();
    return items.filter((item) => item.label.toLowerCase().includes(q));
  }, [filter, fromQueueLabel, shell.data?.sidebar?.sections]);

  return (
    <div className="space-y-3">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Route from {fromQueueLabel}</h3>
        <p className="text-2xs text-muted-foreground">
          {messageIds.length} message{messageIds.length === 1 ? "" : "s"} · applies target label,
          removes queue label{archive ? ", marks read, and archives" : ""}
        </p>
      </div>
      <Input
        autoFocus
        aria-label="Filter route targets"
        placeholder="Filter route targets"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <div className="space-y-1">
        {targets.length === 0 ? (
          <div className="text-xs text-muted-foreground">No labels.</div>
        ) : (
          targets.map((label) => (
            <RouteRow
              key={label.id}
              label={label.label}
              messageIds={messageIds}
              fromQueueLabel={fromQueueLabel}
              archive={archive}
              onRouted={onClose}
            />
          ))
        )}
      </div>
    </div>
  );
}

function RouteRow({
  label,
  messageIds,
  fromQueueLabel,
  archive,
  onRouted,
}: {
  label: string;
  messageIds: string[];
  fromQueueLabel: string;
  archive: boolean;
  onRouted: () => void;
}) {
  const mutation = useOptimisticMailMutation("route", {
    payload: { label, fromQueueLabel, archive },
  });
  return (
    <Button
      variant="ghost"
      className="h-8 w-full justify-start text-xs"
      disabled={mutation.isPending}
      onClick={() => {
        mutation.mutate(messageIds, { onSuccess: onRouted });
      }}
    >
      {label}
    </Button>
  );
}
