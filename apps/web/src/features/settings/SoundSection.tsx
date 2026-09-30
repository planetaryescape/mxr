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
  type ChimesPatch,
} from "@/features/sound/api";
import { previewSound } from "@/features/sound/player";
import type { Voice } from "@/features/sound/voices";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
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
  // Each change is a patch of the fields it touches, applied by the daemon
  // to the setting as it is then. The screen shows every change at once;
  // an answer only lands if no newer change has been made since, so a slow
  // reply can never put an older value back.
  const latestChange = useRef(0);
  const save = useMutation({
    mutationFn: ({ patch }: { patch: ChimesPatch; change: number; before?: ChimeSettings }) =>
      saveChimeSettings(patch),
    scope: { id: "notification-chimes" },
    onSuccess: (saved, { change }) => {
      if (change === latestChange.current) {
        queryClient.setQueryData(chimeSettingsQuery.queryKey, saved);
      }
    },
    onError: (error, { change, before }) => {
      toast.error("Couldn't save the sound setting", { description: error.message });
      // Put back what was shown before this change, unless a newer change
      // will say, then ask the daemon. The put-back doesn't wait for the
      // refetch, which pauses while the daemon is unreachable.
      if (change === latestChange.current) {
        if (before) queryClient.setQueryData(chimeSettingsQuery.queryKey, before);
        void queryClient.invalidateQueries({ queryKey: chimeSettingsQuery.queryKey });
      }
    },
  });
  const update = (patch: ChimesPatch) => {
    // Refused before the optimistic write, so the screen never shows a
    // setting the daemon didn't take.
    if (refuseWhileDaemonDown("change the sound setting")) return;
    latestChange.current += 1;
    const before = queryClient.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey);
    queryClient.setQueryData<ChimeSettings>(chimeSettingsQuery.queryKey, (current) =>
      current ? { ...current, ...definedFields(patch) } : current,
    );
    save.mutate({ patch, change: latestChange.current, before });
  };

  const config = settings.data;
  const [volume, setVolume] = useState<number | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
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

  const shownVolume = volume ?? config.volume;
  const onVolume = (next: number) => {
    setVolume(next);
    if (volumeTimer.current) clearTimeout(volumeTimer.current);
    volumeTimer.current = setTimeout(() => {
      update({ volume: next });
      setVolume(null);
    }, VOLUME_SAVE_DELAY_MS);
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
                onValueChange={(next) => update(soundPatch(event.key, next as ChimeSoundName))}
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

/** The patch's set fields only, so an absent field never blanks the cache. */
function definedFields(patch: ChimesPatch): Partial<ChimeSettings> {
  return Object.fromEntries(
    Object.entries(patch).filter(([, value]) => value !== undefined && value !== null),
  ) as Partial<ChimeSettings>;
}

function soundPatch(key: (typeof EVENTS)[number]["key"], sound: ChimeSoundName): ChimesPatch {
  const patch: ChimesPatch = {};
  patch[key] = sound;
  return patch;
}
