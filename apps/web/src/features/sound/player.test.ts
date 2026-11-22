import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveQueryClient } from "@/lib/queryClient";

import { chimeSettingsQuery, type ChimeSettings } from "./api";
import { installSoundPlayer, playSound, resetSoundPlayer } from "./player";

type Call = [method: string, ...args: unknown[]];

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
});
