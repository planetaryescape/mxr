/*
 * Send-later dialog: natural-language time input (parseSendLater) with a
 * live preview of the parsed time, plus a few presets. Confirm hands the
 * resolved Date back to the compose controller, which materialises the
 * session into a stored draft and schedules it.
 */

import { Clock, Loader2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

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
import { parseSendLater } from "./sendLater";

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
}

export function SendLaterDialog({
  open,
  onOpenChange,
  scheduling,
  onConfirm,
}: SendLaterDialogProps) {
  const [input, setInput] = useState("");

  useEffect(() => {
    if (!open) setInput("");
  }, [open]);

  const parsed = useMemo(() => parseSendLater(input), [input]);

  function confirm(text: string) {
    if (scheduling) return;
    const result = parseSendLater(text);
    if (!result) return;
    onConfirm(result.at, result.label);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Send later</DialogTitle>
          <DialogDescription>
            Schedule this message instead of sending it now. The draft is stored locally and
            dispatched by the daemon.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-2">
          {PRESETS.map((preset) => {
            const presetParse = parseSendLater(preset.input);
            return (
              <Button
                key={preset.input}
                variant="outline"
                className="h-auto justify-start rounded-lg px-3 py-2 text-left"
                onClick={() => confirm(preset.input)}
                disabled={!presetParse || scheduling}
              >
                <Clock className="size-3.5" />
                <span className="grid gap-0.5">
                  <span className="text-xs font-medium">{preset.label}</span>
                  {presetParse ? (
                    <span className="font-mono text-2xs text-muted-foreground">
                      {presetParse.label}
                    </span>
                  ) : null}
                </span>
              </Button>
            );
          })}
        </div>

        <div className="space-y-2 rounded-xl border border-border bg-muted/40 p-3">
          <Label htmlFor="send-later-time">Custom send time</Label>
          <Input
            id="send-later-time"
            value={input}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                confirm(input);
              }
            }}
            placeholder="tomorrow 9am, in 2h, monday 17:00"
            autoFocus
          />
          <div className="text-2xs" role="status">
            {parsed ? (
              <span className="text-success">Sends {parsed.label}</span>
            ) : input.trim() ? (
              <span className="text-muted-foreground">
                Can&apos;t read that time yet — try &quot;in 2h&quot; or &quot;tomorrow 9am&quot;.
              </span>
            ) : (
              <span className="text-muted-foreground">
                Examples: in 2h, tomorrow 9am, monday 17:00.
              </span>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={scheduling}>
            Cancel
          </Button>
          <Button onClick={() => confirm(input)} disabled={!parsed || scheduling}>
            {scheduling ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Clock className="size-3" />
            )}
            Schedule send
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
