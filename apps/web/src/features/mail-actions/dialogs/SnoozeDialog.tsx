import { useQuery } from "@tanstack/react-query";
import { Clock } from "lucide-react";
import { useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
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
import { fetchSnoozePresets, type SnoozePreset } from "@/features/mailbox/api";
import { formatLongDate, plural } from "@/lib/format";

import { performMailAction } from "../mailMutations";

interface SnoozeDialogProps {
  open: boolean;
  messageIds: string[];
  subject?: string;
  onOpenChange: (open: boolean) => void;
  onSnoozed?: () => void;
}

/**
 * Presets from the daemon's config (TUI parity), picked with 1-9, plus
 * natural-language times the daemon parses ("in 2h", "monday 9am").
 */
export function SnoozeDialog({
  open,
  messageIds,
  subject,
  onOpenChange,
  onSnoozed,
}: SnoozeDialogProps) {
  const [custom, setCustom] = useState("");
  const customRef = useRef<HTMLInputElement>(null);
  const presets = useQuery({
    queryKey: ["snooze-presets"],
    queryFn: fetchSnoozePresets,
    enabled: open,
    staleTime: 0,
  });
  const choices = (presets.data?.presets ?? []).filter(isDisplayablePreset);

  function snooze(until: string) {
    const value = until.trim();
    if (!value || messageIds.length === 0) return;
    onOpenChange(false);
    setCustom("");
    onSnoozed?.();
    void performMailAction("snooze", messageIds, { until: value });
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setCustom("");
        onOpenChange(next);
      }}
    >
      <DialogContent
        // Holding focus itself is a keyboard detail, not a place to ring.
        className="max-w-md outline-none"
        // The dialog itself takes focus, not the time field (the only
        // focusable thing while presets load), so Z then 1 picks a preset.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (event.currentTarget as HTMLElement | null)?.focus();
        }}
        onKeyDown={(event) => {
          if (event.target instanceof HTMLInputElement) return;
          if (event.metaKey || event.ctrlKey || event.altKey) return;
          if (/^[1-9]$/.test(event.key)) {
            event.preventDefault();
            const preset = choices[Number(event.key) - 1];
            if (preset) snooze(presetValue(preset));
            return;
          }
          // Any other typing is a time: send it to the field.
          if (event.key.length === 1 && event.key !== " ") customRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Snooze until…</DialogTitle>
          <DialogDescription className="truncate">
            {subject ?? plural(messageIds.length, "message")} leaves the inbox and comes back then.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-1" role="group" aria-label="Snooze presets">
          {presets.isLoading
            ? Array.from({ length: 4 }, (_, index) => (
                <div key={index} className="h-11 animate-pulse rounded-md bg-muted/60" />
              ))
            : null}
          {presets.isError ? (
            <p className="text-sm text-destructive">
              Couldn't load presets: {presets.error.message}
            </p>
          ) : null}
          {choices.map((preset, index) => {
            const wake = preset.wakeAt ?? preset.wake_at;
            return (
              <button
                key={`${presetLabel(preset)}-${wake ?? index}`}
                type="button"
                onClick={() => snooze(presetValue(preset))}
                className="flex items-center gap-3 rounded-md px-3 py-2 text-left hover:bg-accent focus-visible:bg-accent"
              >
                <Clock className="size-4 text-muted-foreground" />
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] font-medium">{presetLabel(preset)}</span>
                  {wake ? (
                    <span className="block font-mono text-2xs text-muted-foreground">
                      {formatLongDate(wake)}
                    </span>
                  ) : null}
                </span>
                {index < 9 ? <KeyChip>{index + 1}</KeyChip> : null}
              </button>
            );
          })}
        </div>

        <form
          className="grid gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            snooze(custom);
          }}
        >
          <label htmlFor="snooze-custom" className="text-[13px] font-medium">
            Or type a time
          </label>
          <div className="flex gap-2">
            <Input
              id="snooze-custom"
              ref={customRef}
              value={custom}
              onChange={(event) => setCustom(event.target.value)}
              placeholder="in 2h, tomorrow 9am, monday 17:00"
            />
            <Button type="submit" disabled={!custom.trim()}>
              Snooze
            </Button>
          </div>
        </form>
        <DialogFooter className="text-2xs text-muted-foreground sm:justify-start">
          Undo with u for about a minute afterwards.
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function presetLabel(preset: SnoozePreset): string {
  return preset.label ?? preset.name ?? "Preset";
}

function presetValue(preset: SnoozePreset): string {
  return preset.id ?? preset.name ?? preset.label ?? "";
}

/** "Tonight" is meaningless after tonight has passed. */
function isDisplayablePreset(preset: SnoozePreset): boolean {
  if (!presetValue(preset)) return false;
  if (presetLabel(preset).trim().toLowerCase() !== "tonight") return true;
  const wake = preset.wakeAt ?? preset.wake_at;
  if (!wake) return true;
  const date = new Date(wake);
  if (Number.isNaN(date.getTime())) return true;
  return date.toDateString() === new Date().toDateString();
}
