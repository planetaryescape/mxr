import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Clock, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { fetchSnoozePresets, shellKey, snoozeMessages } from "./api";
import { Alert } from "@/components/ui/alert";
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
import { Label } from "@/components/ui/label";

interface SnoozeDialogProps {
  open: boolean;
  messageIds: string[];
  onOpenChange: (open: boolean) => void;
  onSnoozed?: () => void;
}

export function SnoozeDialog({ open, messageIds, onOpenChange, onSnoozed }: SnoozeDialogProps) {
  const [customUntil, setCustomUntil] = useState("");
  const qc = useQueryClient();
  const presets = useQuery({
    queryKey: ["snooze-presets"],
    queryFn: fetchSnoozePresets,
    enabled: open,
    staleTime: 0,
    refetchOnMount: "always",
  });
  const snooze = useMutation({
    mutationFn: (until: string) => snoozeMessages(messageIds, until),
    onSuccess: () => {
      toast.success(
        `Snoozed ${messageIds.length} ${messageIds.length === 1 ? "message" : "messages"}`,
      );
      setCustomUntil("");
      onOpenChange(false);
      onSnoozed?.();
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
      void qc.invalidateQueries({ queryKey: ["thread"] });
      void qc.invalidateQueries({ queryKey: shellKey });
    },
    onError: (error) => toast.error("Snooze failed", { description: error.message }),
  });

  useEffect(() => {
    if (!open) setCustomUntil("");
  }, [open]);

  function run(until: string) {
    const trimmed = until.trim();
    if (!trimmed || messageIds.length === 0 || snooze.isPending) return;
    snooze.mutate(trimmed);
  }

  const countLabel = `${messageIds.length} ${messageIds.length === 1 ? "message" : "messages"}`;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Snooze {countLabel}</DialogTitle>
          <DialogDescription>
            Move selected mail out of the mailbox until a preset or custom time.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-2">
          {presets.isLoading ? (
            <Alert role="status" variant="muted" className="px-3 py-2 text-xs">
              Loading snooze presets...
            </Alert>
          ) : null}
          {(presets.data?.presets ?? []).filter(isDisplayablePreset).map((preset) => {
            const until = preset.id ?? preset.name ?? preset.label ?? "";
            const label = preset.label ?? preset.name ?? "Preset";
            return (
              <Button
                key={`${label}-${preset.wakeAt ?? preset.wake_at ?? ""}`}
                variant="outline"
                className="h-auto justify-start rounded-lg px-3 py-2 text-left"
                onClick={() => run(until)}
                disabled={!until || snooze.isPending}
              >
                <Clock className="size-3.5" />
                <span className="grid gap-0.5">
                  <span className="text-xs font-medium">{label}</span>
                  {preset.wakeAt || preset.wake_at ? (
                    <span className="font-mono text-2xs text-muted-foreground">
                      {formatWakeAt(preset.wakeAt ?? preset.wake_at)}
                    </span>
                  ) : null}
                </span>
              </Button>
            );
          })}
          {presets.isError ? (
            <Alert variant="destructive" className="px-3 py-2 text-xs">
              {presets.error.message}
            </Alert>
          ) : null}
        </div>

        <div className="space-y-2 rounded-xl border border-border bg-muted/40 p-3">
          <Label htmlFor="snooze-custom-time">Custom snooze time</Label>
          <Input
            id="snooze-custom-time"
            value={customUntil}
            onChange={(event) => setCustomUntil(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                run(customUntil);
              }
            }}
            placeholder="tomorrow 9am, in 2h, monday 17:00"
          />
          <div className="text-2xs text-muted-foreground">
            Examples: in 2h, tomorrow 9am, monday 17:00.
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={snooze.isPending}>
            Cancel
          </Button>
          <Button
            onClick={() => run(customUntil)}
            disabled={!customUntil.trim() || snooze.isPending}
          >
            {snooze.isPending ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Clock className="size-3" />
            )}
            Snooze custom time
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatWakeAt(value?: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function isDisplayablePreset(preset: {
  label?: string;
  name?: string;
  wakeAt?: string;
  wake_at?: string;
}): boolean {
  const label = (preset.label ?? preset.name ?? "").trim().toLowerCase();
  if (label !== "tonight") return true;
  const wakeAt = preset.wakeAt ?? preset.wake_at;
  if (!wakeAt) return true;
  const wakeDate = new Date(wakeAt);
  if (Number.isNaN(wakeDate.getTime())) return true;
  const today = new Date();
  return (
    wakeDate.getFullYear() === today.getFullYear() &&
    wakeDate.getMonth() === today.getMonth() &&
    wakeDate.getDate() === today.getDate()
  );
}
