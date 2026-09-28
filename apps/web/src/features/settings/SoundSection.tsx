import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { SettingRow, ToggleSetting } from "./settingsParts";
import { Button } from "@/components/ui/button";
import {
  chimeSettingsQuery,
  saveChimeSettings,
  type ChimeSettings,
  type ChimeSoundName,
} from "@/features/sound/api";
import { previewSound } from "@/features/sound/player";
import type { Voice } from "@/features/sound/voices";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/** The events the web plays, in the words of the setting. */
const EVENTS = [
  { key: "sent", label: "Send", description: "When a message goes out." },
  { key: "archived", label: "Archive", description: "Archive and sweep, once per action." },
  { key: "snoozed", label: "Snooze", description: "When a conversation is put off." },
] as const satisfies readonly { key: keyof ChimeSettings; label: string; description: string }[];

const SOUND_OPTIONS: { value: ChimeSoundName; label: string }[] = [
  { value: "sent", label: "Sent" },
  { value: "archive", label: "Archive" },
  { value: "pop", label: "Pop" },
  { value: "glass", label: "Glass" },
  { value: "bell", label: "Bell" },
  { value: "thud", label: "Thud" },
  { value: "alert", label: "Alert" },
  { value: "none", label: "None" },
];

/** Volume saves after the slider rests, not on every step of a drag. */
const VOLUME_SAVE_DELAY_MS = 300;

export function SoundSection() {
  const queryClient = useQueryClient();
  const settings = useQuery(chimeSettingsQuery);
  const save = useMutation({
    mutationFn: saveChimeSettings,
    // One save at a time, in order: a late answer can't undo a newer change.
    scope: { id: "notification-chimes" },
    onMutate: (next) => {
      const previous = queryClient.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey);
      queryClient.setQueryData(chimeSettingsQuery.queryKey, next);
      return { previous };
    },
    onError: (error, _next, context) => {
      queryClient.setQueryData(chimeSettingsQuery.queryKey, context?.previous);
      toast.error("Couldn't save the sound setting", { description: error.message });
    },
    onSuccess: (saved) => queryClient.setQueryData(chimeSettingsQuery.queryKey, saved),
  });

  const config = settings.data;
  const [volume, setVolume] = useState<number | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** A slider value not saved yet; any save carries it along. */
  const pendingVolume = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (volumeTimer.current) clearTimeout(volumeTimer.current);
    },
    [],
  );

  if (settings.isLoading) {
    return <p className="py-3 text-[13px] text-muted-foreground">Loading the sound setting…</p>;
  }
  if (!config) {
    return (
      <p className="py-3 text-[13px] text-muted-foreground">
        Couldn't read the sound setting from the daemon.{" "}
        <button type="button" className="underline" onClick={() => void settings.refetch()}>
          Try again
        </button>
      </p>
    );
  }

  // The daemon takes the whole setting, so every save starts from the latest
  // one (the cache holds each change as it is made), never from this render's.
  const update = (patch: Partial<ChimeSettings>) => {
    if (volumeTimer.current) clearTimeout(volumeTimer.current);
    volumeTimer.current = null;
    const latest = queryClient.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey) ?? config;
    const unsaved = pendingVolume.current;
    pendingVolume.current = null;
    setVolume(null);
    save.mutate({ ...latest, ...(unsaved === null ? {} : { volume: unsaved }), ...patch });
  };
  const shownVolume = volume ?? config.volume;
  const onVolume = (next: number) => {
    setVolume(next);
    pendingVolume.current = next;
    if (volumeTimer.current) clearTimeout(volumeTimer.current);
    volumeTimer.current = setTimeout(() => update({}), VOLUME_SAVE_DELAY_MS);
  };
  const preview = (voice: Voice) => previewSound(voice, shownVolume);

  return (
    <div>
      <ToggleSetting
        id="sound-enabled"
        label="Sound"
        description="Short, soft tones made in the browser. Never for moving around, never in a background tab. The same setting plays in the terminal app."
        checked={config.enabled}
        onChange={(enabled) => update({ enabled })}
      />
      <SettingRow
        label="Volume"
        htmlFor="sound-volume"
        description={`${Math.round(shownVolume * 100)}%`}
      >
        <input
          id="sound-volume"
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={shownVolume}
          onChange={(event) => onVolume(Number(event.target.value))}
          className="w-48 accent-primary"
        />
      </SettingRow>
      {EVENTS.map((event) => {
        const sound = config[event.key];
        return (
          <SettingRow key={event.key} label={event.label} description={event.description}>
            <div className="flex items-center gap-2">
              <Select
                value={sound}
                onValueChange={(next) => update({ [event.key]: next as ChimeSoundName })}
              >
                <SelectTrigger className="h-8 w-32 text-xs" aria-label={`${event.label} sound`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SOUND_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value} className="text-xs">
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`Play the ${event.label.toLowerCase()} sound`}
                disabled={sound === "none"}
                onClick={() => sound !== "none" && preview(sound)}
              >
                <Play />
              </Button>
            </div>
          </SettingRow>
        );
      })}
      <SettingRow label="Cleared desk" description="Two soft notes when the desk becomes clear.">
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Play the cleared desk sound"
          onClick={() => preview("low_tide")}
        >
          <Play />
        </Button>
      </SettingRow>
    </div>
  );
}
