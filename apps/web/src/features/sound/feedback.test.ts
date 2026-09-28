import { afterEach, describe, expect, it, vi } from "vitest";

import { QueryClient } from "@tanstack/react-query";

import { emitSendEvent } from "@/features/compose/session/sendEvents";
import { setActiveQueryClient } from "@/lib/queryClient";

const playSound = vi.fn<(event: string) => boolean>(() => true);
vi.mock("./player", () => ({
  installSoundPlayer: () => () => undefined,
  playSound: (event: string) => playSound(event),
}));

const { installSoundFeedback, playMailActionSound } = await import("./feedback");

afterEach(() => playSound.mockClear());

describe("sound feedback", () => {
  it("plays once when a send goes out, not when it is queued or undone", () => {
    const stop = installSoundFeedback();
    const id = { intentKey: "reply:1", sendId: "reply:1@1" };
    emitSendEvent({ kind: "queued", ...id });
    emitSendEvent({ kind: "cancelled", ...id });
    expect(playSound).not.toHaveBeenCalled();
    emitSendEvent({ kind: "sent", ...id });
    expect(playSound).toHaveBeenCalledExactlyOnceWith("sent");
    stop();
    emitSendEvent({ kind: "sent", ...id });
    expect(playSound).toHaveBeenCalledTimes(1);
  });

  it("gives archive and snooze a sound and leaves the rest silent", () => {
    playMailActionSound("archive");
    playMailActionSound("read-and-archive");
    playMailActionSound("snooze");
    playMailActionSound("star");
    playMailActionSound("read");
    expect(playSound.mock.calls.map(([event]) => event)).toEqual([
      "archived",
      "archived",
      "snoozed",
    ]);
  });

  it("rereads the shared setting when the page comes back into view", () => {
    const client = new QueryClient();
    const prefetch = vi.spyOn(client, "prefetchQuery").mockResolvedValue(undefined);
    setActiveQueryClient(client);
    const stop = installSoundFeedback();
    window.dispatchEvent(new Event("focus"));
    document.dispatchEvent(new Event("visibilitychange"));
    expect(prefetch).toHaveBeenCalledTimes(2);
    expect(prefetch.mock.calls[0]?.[0]).toMatchObject({
      queryKey: ["notification-chimes"],
      staleTime: 0,
    });
    stop();
    window.dispatchEvent(new Event("focus"));
    expect(prefetch).toHaveBeenCalledTimes(2);
  });
});
