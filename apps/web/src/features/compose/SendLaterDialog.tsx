/*
 * Send-later dialog: a natural-language time field resolved live by the
 * daemon (the parser the CLI and TUI use), plus a few presets that show the
 * time they resolve to. Confirm hands the previewed instant back to the
 * compose controller, which materialises the session into a stored draft and
 * schedules it. The same dialog, with its copy swapped, picks a custom time
 * for "send and remind me".
 */

import { Clock, Loader2 } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { describeChoice, type TimeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime, useResolvedPresets } from "@/features/time/useNaturalTime";

const PRESETS = [
  { label: "Tomorrow 9am", input: "tomorrow 9am" },
  { label: "Monday 9am", input: "monday 9am" },
  { label: "In 2 hours", input: "in 2 hours" },
] as const;

interface SendLaterDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  scheduling: boolean;
  onConfirm: (at: Date, label: string) => void;
  title?: string;
  description?: string;
  confirmLabel?: string;
  presets?: readonly { label: string; input: string }[];
}

export function SendLaterDialog({
  open,
  onOpenChange,
  scheduling,
  onConfirm,
  title = "Send later",
  description = "Schedule this message instead of sending it now. The draft is stored locally and dispatched by the daemon.",
  confirmLabel = "Schedule send",
  presets = PRESETS,
}: SendLaterDialogProps) {
  const time = useNaturalTime({ enabled: open });
  const { reset } = time;

  useEffect(() => {
    if (!open) reset();
  }, [open, reset]);

  const presetTimes = useResolvedPresets(presets, { enabled: open });

  function confirm(choice: TimeChoice | null | undefined) {
    if (scheduling || !choice) return;
    onConfirm(new Date(choice.at), describeChoice(choice));
  }

  async function confirmTyped() {
    confirm(await time.commit());
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="grid gap-2">
          {presets.map((preset, index) => {
            const choice = presetTimes[index]?.choice;
            return (
              <Button
                key={preset.input}
                variant="outline"
                className="h-auto justify-start rounded-lg px-3 py-2 text-left"
                onClick={() => confirm(choice)}
                disabled={!choice || scheduling}
              >
                <Clock className="size-3.5" />
                <span className="grid gap-0.5">
                  <span className="text-xs font-medium">{preset.label}</span>
                  {choice ? (
                    <span className="font-mono text-2xs text-muted-foreground tabular-nums">
                      {describeChoice(choice)}
                    </span>
                  ) : null}
                </span>
              </Button>
            );
          })}
        </div>

        <div className="space-y-2 rounded-xl border border-border bg-muted/40 p-3">
          <Label htmlFor="send-later-time">Or type a time</Label>
          <NaturalTimeInput
            id="send-later-time"
            state={time}
            onCommit={confirm}
            placeholder="fri 3, tomorrow 9am, in 2d"
            autoFocus
          />
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={scheduling}>
            Cancel
          </Button>
          <Button onClick={() => void confirmTyped()} disabled={!time.canCommit || scheduling}>
            {scheduling ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Clock className="size-3" />
            )}
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
