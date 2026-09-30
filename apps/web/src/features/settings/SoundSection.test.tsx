import { onlineManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ChimeSettings, ChimesPatch } from "@/features/sound/api";
import { useConnectionStore } from "@/state/connectionStore";

const SETTINGS: ChimeSettings = {
  enabled: false,
  volume: 0.35,
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

/**
 * A daemon that applies each patch when it arrives and answers only when
 * the test says so, so answers can come back late.
 */
let server: ChimeSettings = { ...SETTINGS };
const pending: { answer: () => void }[] = [];
const saveChimeSettings = vi.fn<(patch: ChimesPatch) => Promise<ChimeSettings>>(
  (patch) =>
    new Promise((resolve) => {
      server = { ...server, ...(patch as Partial<ChimeSettings>) };
      const snapshot = { ...server };
      pending.push({ answer: () => resolve(snapshot) });
    }),
);
vi.mock("@/features/sound/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/sound/api")>()),
  fetchChimeSettings: async () => server,
  saveChimeSettings: (patch: ChimesPatch) => saveChimeSettings(patch),
}));

const { SoundSection } = await import("./SoundSection");
const { chimeSettingsQuery } = await import("@/features/sound/api");
const { createQueryClient } = await import("@/lib/queryClient");

async function answerNext() {
  await waitFor(() => expect(pending.length).toBeGreaterThan(0));
  await act(async () => pending.shift()!.answer());
}

// Radix Select reads these browser APIs, which jsdom leaves out.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.scrollIntoView ??= () => undefined;

describe("SoundSection", () => {
  it("interleaved changes each send only their field, and a late answer never undoes a newer one", async () => {
    const client = new QueryClient();
    client.setQueryData(chimeSettingsQuery.queryKey, SETTINGS);
    render(
      <QueryClientProvider client={client}>
        <SoundSection />
      </QueryClientProvider>,
    );
    const shown = () => client.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey);

    // 1. Turn sound on; its save is in flight.
    fireEvent.click(await screen.findByRole("switch", { name: "Sound" }));
    await waitFor(() => expect(saveChimeSettings).toHaveBeenCalledTimes(1));
    // 2. Move the volume; its save waits behind the first.
    fireEvent.change(screen.getByLabelText("Volume"), { target: { value: "0.8" } });
    await waitFor(() => expect(shown()?.volume).toBe(0.8), { timeout: 1000 });
    // 3. The toggle's answer arrives late, still saying volume 0.35.
    await answerNext();
    expect(shown()).toMatchObject({ enabled: true, volume: 0.8 });
    // 4. Pick a new sound for snooze.
    fireEvent.keyDown(screen.getByRole("combobox", { name: "Snooze sound" }), { key: "Enter" });
    fireEvent.keyDown(await screen.findByRole("option", { name: "Glass" }), { key: "Enter" });
    expect(shown()).toMatchObject({ enabled: true, volume: 0.8, snoozed: "glass" });
    await answerNext();
    expect(shown()).toMatchObject({ enabled: true, volume: 0.8, snoozed: "glass" });
    await answerNext();

    expect(saveChimeSettings.mock.calls.map(([patch]) => patch)).toEqual([
      { enabled: true },
      { volume: 0.8 },
      { snoozed: "glass" },
    ]);
    expect(shown()).toEqual({ ...SETTINGS, enabled: true, volume: 0.8, snoozed: "glass" });
    expect(server).toEqual(shown());
  });

  it("while the daemon is down a change is refused before the screen shows it", async () => {
    saveChimeSettings.mockClear();
    const client = new QueryClient();
    client.setQueryData(chimeSettingsQuery.queryKey, SETTINGS);
    useConnectionStore.setState({ daemonDown: true });
    try {
      render(
        <QueryClientProvider client={client}>
          <SoundSection />
        </QueryClientProvider>,
      );
      const toggle = await screen.findByRole("switch", { name: "Sound" });
      fireEvent.click(toggle);
      expect(client.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey)?.enabled).toBe(false);
      expect(saveChimeSettings).not.toHaveBeenCalled();
    } finally {
      useConnectionStore.setState({ daemonDown: false });
    }
  });

  it("a failed save puts the old value back without waiting on a refetch", async () => {
    saveChimeSettings.mockClear();
    saveChimeSettings.mockImplementationOnce(async () => {
      throw new Error("Couldn't reach mxr's daemon.");
    });
    // The app's client: mutations don't pause while offline.
    const client = createQueryClient();
    client.setQueryData(chimeSettingsQuery.queryKey, SETTINGS);
    // Refetches pause, as they do while the daemon is unreachable.
    onlineManager.setOnline(false);
    try {
      render(
        <QueryClientProvider client={client}>
          <SoundSection />
        </QueryClientProvider>,
      );
      fireEvent.click(await screen.findByRole("switch", { name: "Sound" }));
      await waitFor(() => expect(saveChimeSettings).toHaveBeenCalledTimes(1));
      await waitFor(() =>
        expect(client.getQueryData<ChimeSettings>(chimeSettingsQuery.queryKey)?.enabled).toBe(
          false,
        ),
      );
    } finally {
      onlineManager.setOnline(true);
    }
  });
});
