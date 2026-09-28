import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveQueryClient } from "@/lib/queryClient";

import { chimeSettingsQuery, type ChimeSettings } from "./api";
import {
  installSoundPlayer,
  playSound,
  resetSoundPlayer,
  scheduleVoice,
  voiceFor,
  type SoundContext,
} from "./player";
import { MAX_VOICE_SECONDS, VOICES } from "./voices";

type Call = [method: string, ...args: unknown[]];

/** A fake AudioContext that records every scheduling call. */
function fakeContext(state: AudioContextState = "running") {
  const calls: Call[] = [];
  const param = (name: string) => ({
    setValueAtTime: (value: number, at: number) => calls.push([`${name}.set`, value, at]),
    exponentialRampToValueAtTime: (value: number, at: number) =>
      calls.push([`${name}.ramp`, value, at]),
  });
  const context = {
    state,
    currentTime: 10,
    destination: {},
    resume: vi.fn<() => Promise<void>>(async () => {
      context.state = "running";
    }),
    createOscillator: () => ({
      type: "sine",
      frequency: param("freq"),
      connect: () => calls.push(["osc.connect"]),
      start: (at: number) => calls.push(["osc.start", at]),
      stop: (at: number) => calls.push(["osc.stop", at]),
    }),
    createGain: () => ({
      gain: param("gain"),
      connect: () => calls.push(["gain.connect"]),
    }),
  };
  return { context, calls };
}

function peak(calls: Call[]): unknown {
  return calls.find(([method]) => method === "gain.ramp")?.[1];
}

const SETTINGS: ChimeSettings = {
  enabled: true,
  volume: 0.5,
  new_mail: "bell",
  sent: "sent",
  archived: "archive",
  trashed: "thud",
  spam: "alert",
  snoozed: "pop",
  unsnoozed: "glass",
  reminder: "bell",
  error: "alert",
};

function useSettings(settings: ChimeSettings | undefined) {
  const client = new QueryClient();
  if (settings) client.setQueryData(chimeSettingsQuery.queryKey, settings);
  setActiveQueryClient(client);
}

describe("scheduleVoice", () => {
  it("starts and ends every partial at silence on exponential curves", () => {
    const { context, calls } = fakeContext();
    const scheduled = scheduleVoice(context as unknown as SoundContext, VOICES.sent, 0.5, 10);
    expect(scheduled).toBe(VOICES.sent.length);
    const gains = calls.filter(([method]) => method === "gain.set" || method === "gain.ramp");
    // Per partial: silent start, ramp up, ramp back down to silence.
    for (let index = 0; index < gains.length; index += 3) {
      const [set, up, down] = gains.slice(index, index + 3);
      expect(set).toEqual(["gain.set", 0.0001, expect.any(Number)]);
      expect(up?.[0]).toBe("gain.ramp");
      expect(up?.[1]).toBeGreaterThan(0.0001);
      expect(up?.[1]).toBeLessThanOrEqual(0.2);
      expect(down).toEqual(["gain.ramp", 0.0001, expect.any(Number)]);
    }
    // Never a linear ramp or a hard set to zero, which click.
    expect(calls.some(([method, value]) => method === "gain.set" && value === 0)).toBe(false);
  });

  it("keeps every voice short", () => {
    for (const voice of Object.values(VOICES)) {
      const end = Math.max(...voice.map((partial) => partial.at + partial.length));
      expect(end).toBeLessThanOrEqual(MAX_VOICE_SECONDS);
    }
  });

  it("scales with volume and schedules nothing at zero", () => {
    const loud = fakeContext();
    scheduleVoice(loud.context as unknown as SoundContext, VOICES.archive, 1, 0);
    const quiet = fakeContext();
    scheduleVoice(quiet.context as unknown as SoundContext, VOICES.archive, 0.25, 0);
    expect(peak(quiet.calls)).toBeCloseTo(Number(peak(loud.calls)) / 4);

    const silent = fakeContext();
    expect(scheduleVoice(silent.context as unknown as SoundContext, VOICES.archive, 0, 0)).toBe(0);
    expect(silent.calls).toEqual([]);
  });

  it("gives a cleared desk a rising two-note figure", () => {
    const [first, second] = VOICES.low_tide;
    expect(VOICES.low_tide).toHaveLength(2);
    expect(second!.at).toBeGreaterThan(first!.at);
    expect(second!.from).toBeGreaterThan(first!.from);
  });
});

describe("playSound", () => {
  let fake: ReturnType<typeof fakeContext>;
  let cleanup: () => void;

  beforeEach(() => {
    resetSoundPlayer();
    fake = fakeContext();
    vi.stubGlobal(
      "AudioContext",
      vi.fn<() => unknown>(function FakeAudioContext() {
        return fake.context;
      }),
    );
    cleanup = installSoundPlayer(window);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
  });

  it("stays silent while sound is off, which is the default", () => {
    useSettings({ ...SETTINGS, enabled: false });
    expect(playSound("archived")).toBe(false);
    useSettings(undefined);
    expect(playSound("archived")).toBe(false);
    expect(AudioContext).not.toHaveBeenCalled();
  });

  it("plays the sound the setting names for the event", async () => {
    useSettings(SETTINGS);
    expect(playSound("archived")).toBe(true);
    await vi.waitFor(() =>
      expect(fake.calls.some(([method]) => method === "osc.start")).toBe(true),
    );
    expect(voiceFor("archived", SETTINGS)).toBe("archive");
    expect(voiceFor("snoozed", SETTINGS)).toBe("pop");
    expect(voiceFor("sent", { ...SETTINGS, sent: "none" })).toBeNull();
  });

  it("never plays in a background tab", () => {
    useSettings(SETTINGS);
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    expect(playSound("sent")).toBe(false);
  });

  it("plays once for a held key, not for its repeats", () => {
    useSettings(SETTINGS);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e" }));
    expect(playSound("archived")).toBe(true);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", repeat: true }));
    expect(playSound("archived")).toBe(false);
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "e" }));
    expect(playSound("archived")).toBe(true);
  });

  it("skips silently when the browser won't start audio", async () => {
    useSettings(SETTINGS);
    fake.context.state = "suspended";
    fake.context.resume.mockImplementation(() => new Promise(() => undefined));
    expect(playSound("sent")).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(fake.calls).toEqual([]);
  });
});
