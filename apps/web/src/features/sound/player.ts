import { getActiveQueryClient } from "@/lib/queryClient";

import { chimeSettingsQuery, type ChimeSettings } from "./api";
import { VOICES, type Partial, type Voice } from "./voices";

export type SoundEvent = "sent" | "archived" | "snoozed" | "low_tide";

const MASTER_LEVEL = 0.2;
const ATTACK_SECONDS = 0.012;
const SILENT = 0.0001;

export type SoundContext = Pick<
  AudioContext,
  "currentTime" | "destination" | "createOscillator" | "createGain"
>;

export function scheduleVoice(
  context: SoundContext,
  voice: readonly Partial[],
  volume: number,
  startAt: number,
): number {
  let scheduled = 0;
  for (const partial of voice) {
    const peak = partial.level * clampVolume(volume) * MASTER_LEVEL;
    if (peak <= SILENT) continue;
    const start = startAt + partial.at;
    const end = start + partial.length;
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.type = partial.wave;
    oscillator.frequency.setValueAtTime(partial.from, start);
    if (partial.to) oscillator.frequency.exponentialRampToValueAtTime(partial.to, end);
    gain.gain.setValueAtTime(SILENT, start);
    gain.gain.exponentialRampToValueAtTime(peak, start + ATTACK_SECONDS);
    gain.gain.exponentialRampToValueAtTime(SILENT, end);
    oscillator.connect(gain);
    gain.connect(context.destination);
    oscillator.start(start);
    oscillator.stop(end + 0.02);
    scheduled += 1;
  }
  return scheduled;
}

function clampVolume(volume: number): number {
  return Number.isFinite(volume) ? Math.min(1, Math.max(0, volume)) : 0;
}

let context: AudioContext | null = null;
let keyHeld = false;

function audioContext(): AudioContext | null {
  if (context) return context;
  if (typeof AudioContext === "undefined") return null;
  try {
    context = new AudioContext();
  } catch {
    return null;
  }
  return context;
}

const RESUME_WAIT_MS = 150;

async function playVoice(voice: Voice, volume: number): Promise<boolean> {
  const ctx = audioContext();
  if (!ctx) return false;
  if (!isRunning(ctx)) {
    const resumed = await Promise.race([
      ctx.resume().then(
        () => true,
        () => false,
      ),
      new Promise<boolean>((resolve) => setTimeout(() => resolve(false), RESUME_WAIT_MS)),
    ]);
    if (!resumed || !isRunning(ctx)) return false;
  }
  return scheduleVoice(ctx, VOICES[voice], volume, ctx.currentTime) > 0;
}

function isRunning(ctx: AudioContext): boolean {
  return ctx.state === "running";
}

function currentSettings(): ChimeSettings | undefined {
  return getActiveQueryClient()?.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey);
}

export function voiceFor(event: SoundEvent, settings: ChimeSettings): Voice | null {
  if (event === "low_tide") return "low_tide";
  const sound = settings[event];
  return sound === "none" ? null : sound;
}

export function playSound(event: SoundEvent): boolean {
  const settings = currentSettings();
  if (!settings?.enabled) return false;
  if (typeof document !== "undefined" && document.hidden) return false;
  if (keyHeld) return false;
  const voice = voiceFor(event, settings);
  if (!voice) return false;
  void playVoice(voice, settings.volume);
  return true;
}

export function previewSound(voice: Voice, volume: number): void {
  void playVoice(voice, volume);
}

export function installSoundPlayer(target: Window = window): () => void {
  const unlock = () => {
    if (!currentSettings()?.enabled) return;
    const ctx = audioContext();
    if (ctx && ctx.state !== "running") void ctx.resume().catch(() => undefined);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    keyHeld = event.repeat;
    unlock();
  };
  const onKeyUp = () => {
    keyHeld = false;
  };
  target.addEventListener("pointerdown", unlock, true);
  target.addEventListener("keydown", onKeyDown, true);
  target.addEventListener("keyup", onKeyUp, true);
  return () => {
    target.removeEventListener("pointerdown", unlock, true);
    target.removeEventListener("keydown", onKeyDown, true);
    target.removeEventListener("keyup", onKeyUp, true);
    keyHeld = false;
  };
}

export function resetSoundPlayer(): void {
  context = null;
  keyHeld = false;
}
