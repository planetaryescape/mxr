import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ChimeSettings } from "@/features/sound/api";

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

const saveChimeSettings = vi.fn<(config: ChimeSettings) => Promise<ChimeSettings>>(
  async (config) => config,
);
vi.mock("@/features/sound/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/sound/api")>()),
  fetchChimeSettings: async () => SETTINGS,
  saveChimeSettings: (config: ChimeSettings) => saveChimeSettings(config),
}));

const { SoundSection } = await import("./SoundSection");

describe("SoundSection", () => {
  it("moving the volume then turning sound on saves both, never the old switch", async () => {
    const client = new QueryClient();
    const { chimeSettingsQuery } = await import("@/features/sound/api");
    client.setQueryData(chimeSettingsQuery.queryKey, SETTINGS);
    render(
      <QueryClientProvider client={client}>
        <SoundSection />
      </QueryClientProvider>,
    );
    fireEvent.change(await screen.findByLabelText("Volume"), { target: { value: "0.8" } });
    fireEvent.click(screen.getByRole("switch", { name: "Sound" }));

    await waitFor(() => expect(saveChimeSettings).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 400));
    const saves = saveChimeSettings.mock.calls.map(([config]) => config);
    expect(saves.at(-1)).toMatchObject({ enabled: true, volume: 0.8 });
    expect(saves.some((config) => config.enabled === false)).toBe(false);
    expect(client.getQueryData(chimeSettingsQuery.queryKey)).toMatchObject({
      enabled: true,
      volume: 0.8,
    });
  });
});
