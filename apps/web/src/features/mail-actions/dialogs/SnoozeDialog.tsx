import { useQuery } from "@tanstack/react-query";
import { Clock } from "lucide-react";
import { useRef } from "react";

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
import { fetchSnoozePresets, type SnoozePreset } from "@/features/mailbox/api";
import { browserTimeZone, describeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime } from "@/features/time/useNaturalTime";
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
 * Presets from the daemon's config (TUI parity), picked with 1-9, plus a
 * natural-language time the daemon resolves live ("fri 3", "in 2d"). Both
 * store the exact instant the dialog showed, not the words, so the wake time
 * can't drift between preview and save.
 */
export function SnoozeDialog({
  open,
  messageIds,
  subject,
  onOpenChange,
  onSnoozed,
}: SnoozeDialogProps) {
  const time = useNaturalTime({ enabled: open });
  const customRef = useRef<HTMLInputElement>(null);
  const presets = useQuery({
    queryKey: ["snooze-presets"],
    queryFn: () => fetchSnoozePresets(browserTimeZone()),
    enabled: open,
    staleTime: 0,
  });
  const choices = (presets.data?.presets ?? []).filter(isDisplayablePreset);

  /** `label` is the wake time as shown, so the toast names what was stored. */
  function snooze(until: string, label?: string) {
    const value = until.trim();
    if (!value || messageIds.length === 0) return;
    onOpenChange(false);
    time.reset();
    onSnoozed?.();
    void performMailAction("snooze", messageIds, {
      until: value,
      payload: label ? { untilLabel: label } : undefined,
    });
  }

  async function snoozeTyped() {
    const choice = await time.commit();
    if (choice) snooze(choice.at, describeChoice(choice));
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) time.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent
        // Holding focus itself is a keyboard detail, not a place to ring.
        // minmax(0, 1fr): a long truncated subject must not widen the grid
        // past the dialog and push the Snooze button out of view.
        className="max-w-md grid-cols-[minmax(0,1fr)] outline-none"
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
            if (preset) snooze(presetUntil(preset), presetWhen(preset));
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
                onClick={() => snooze(presetUntil(preset), presetWhen(preset))}
                className="flex items-center gap-3 rounded-md px-3 py-2 text-left hover:bg-accent focus-visible:bg-accent"
              >
                <Clock className="size-4 text-muted-foreground" />
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] font-medium">{presetLabel(preset)}</span>
                  {presetWhen(preset) ? (
                    <span className="block font-mono text-2xs text-muted-foreground">
                      {presetWhen(preset)}
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
            void snoozeTyped();
          }}
        >
          <label htmlFor="snooze-custom" className="text-[13px] font-medium">
            Or type a time
          </label>
          <div className="flex items-start gap-2">
            <NaturalTimeInput
              id="snooze-custom"
              state={time}
              inputRef={customRef}
              onCommit={(choice) => snooze(choice.at, describeChoice(choice))}
              placeholder="fri 3, tomorrow 9am, in 2d"
              className="min-w-0 flex-1"
            />
            <Button type="submit" size="sm" disabled={!time.canCommit}>
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

function presetWhen(preset: SnoozePreset): string | undefined {
  const wake = preset.wakeAt ?? preset.wake_at;
  return preset.description ?? (wake ? formatLongDate(wake) : undefined);
}

/** The instant the row showed, so what the user saw is what is stored. */
function presetUntil(preset: SnoozePreset): string {
  return preset.wakeAt ?? preset.wake_at ?? presetValue(preset);
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
