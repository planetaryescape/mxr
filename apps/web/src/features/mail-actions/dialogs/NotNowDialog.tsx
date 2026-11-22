import { useQuery } from "@tanstack/react-query";
import { Bookmark, Clock } from "lucide-react";
import { useEffect, useRef } from "react";

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
import { browserTimeZone, describeChoice, type TimeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime } from "@/features/time/useNaturalTime";
import { formatLongDate } from "@/lib/format";

import type { NotNowPicker } from "../notNow";
import { definedKeys } from "@/lib/actions/definition";
import { pressedOverlayKey } from "@/lib/keys/overlayKeys";

const OPTION = "data-not-now-option";

export function NotNowDialog({ picker, onClose }: { picker: NotNowPicker; onClose: () => void }) {
  const time = useNaturalTime({ enabled: true });
  const customRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const laterRef = useRef<HTMLButtonElement>(null);
  const presets = useQuery({
    queryKey: ["snooze-presets"],
    queryFn: () => fetchSnoozePresets(browserTimeZone()),
    staleTime: 0,
  });
  const choices = (presets.data?.presets ?? []).filter(isDisplayablePreset);
  const presetKeys = definedKeys("not-now.preset");

  const hasChoices = choices.length > 0;
  useEffect(() => {
    if (picker.later || !hasChoices) return;
    if (document.activeElement !== contentRef.current) return;
    contentRef.current?.querySelector<HTMLElement>(`[${OPTION}]`)?.focus();
  }, [picker.later, hasChoices]);

  function close() {
    onClose();
    time.reset();
  }

  async function setAt(until: string, label: string) {
    const value = until.trim();
    if (!value) return;
    close();
    await picker.at(value, label);
  }

  function runLater() {
    const later = picker.later;
    if (!later) return;
    return time.runExclusive(async () => {
      close();
      await later.run();
    });
  }

  function pickPreset(preset: SnoozePreset) {
    return time.runExclusive(() =>
      setAt(presetUntil(preset), presetWhen(preset) ?? presetLabel(preset)),
    );
  }

  function pickChoice(choice: TimeChoice) {
    return setAt(choice.at, describeChoice(choice));
  }

  function step(delta: 1 | -1) {
    const options = [...(contentRef.current?.querySelectorAll<HTMLElement>(`[${OPTION}]`) ?? [])];
    if (options.length === 0) return;
    const at = options.indexOf(document.activeElement as HTMLElement);
    const next = at < 0 ? (delta > 0 ? 0 : options.length - 1) : at + delta;
    options[Math.min(options.length - 1, Math.max(0, next))]?.focus();
  }

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        ref={contentRef}
        data-testid="not-now-dialog"
        className="max-w-md grid-cols-[minmax(0,1fr)] outline-none"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (laterRef.current ?? (event.currentTarget as HTMLElement | null))?.focus();
        }}
        onKeyDown={(event) => {
          if (event.target instanceof HTMLInputElement) return;
          const down = pressedOverlayKey(event, "overlay.filter-down");
          if (down || pressedOverlayKey(event, "overlay.filter-up")) {
            event.preventDefault();
            step(down ? 1 : -1);
            return;
          }
          if (event.metaKey || event.ctrlKey || event.altKey) return;
          const nth = definedKeys("not-now.preset").indexOf(event.key);
          if (nth >= 0) {
            event.preventDefault();
            const preset = choices[nth];
            if (preset) void pickPreset(preset);
            return;
          }
          if (event.key.length === 1 && event.key !== " ") customRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{picker.title}</DialogTitle>
          <DialogDescription className="truncate">{picker.description}</DialogDescription>
        </DialogHeader>

        <div className="grid gap-1">
          {picker.later ? (
            <button
              ref={laterRef}
              type="button"
              {...{ [OPTION]: "" }}
              data-testid="not-now-later"
              onClick={() => void runLater()}
              className="flex items-center gap-3 rounded-md px-3 py-2 text-left outline-none hover:bg-accent focus-visible:bg-accent"
            >
              <Bookmark className="size-4 text-muted-foreground" />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{picker.later.label}</span>
                <span className="block text-2xs text-muted-foreground">{picker.later.hint}</span>
              </span>
              <KeyChip>↵</KeyChip>
            </button>
          ) : null}
          <div className="grid gap-1" role="group" aria-label="Times">
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
                  {...{ [OPTION]: "" }}
                  onClick={() => void pickPreset(preset)}
                  className="flex items-center gap-3 rounded-md px-3 py-2 text-left outline-none hover:bg-accent focus-visible:bg-accent"
                >
                  <Clock className="size-4 text-muted-foreground" />
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-medium">{presetLabel(preset)}</span>
                    {presetWhen(preset) ? (
                      <span className="block font-mono text-2xs text-muted-foreground">
                        {presetWhen(preset)}
                      </span>
                    ) : null}
                  </span>
                  {presetKeys[index] ? <KeyChip>{presetKeys[index]}</KeyChip> : null}
                </button>
              );
            })}
          </div>
        </div>

        <form
          className="grid gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            void time.commit(pickChoice);
          }}
        >
          <label htmlFor="not-now-custom" className="text-sm font-medium">
            Or type a time
          </label>
          <div className="flex items-start gap-2">
            <NaturalTimeInput
              id="not-now-custom"
              state={time}
              inputRef={customRef}
              onCommit={pickChoice}
              placeholder="fri 3, tomorrow 9am, in 2d"
              className="min-w-0 flex-1"
            />
            <Button type="submit" size="sm" disabled={!time.canCommit}>
              {picker.timeVerb}
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

function presetUntil(preset: SnoozePreset): string {
  return preset.wakeAt ?? preset.wake_at ?? presetValue(preset);
}

function isDisplayablePreset(preset: SnoozePreset): boolean {
  if (!presetValue(preset)) return false;
  if (presetLabel(preset).trim().toLowerCase() !== "tonight") return true;
  const wake = preset.wakeAt ?? preset.wake_at;
  if (!wake) return true;
  const date = new Date(wake);
  if (Number.isNaN(date.getTime())) return true;
  return date.toDateString() === new Date().toDateString();
}
