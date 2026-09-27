import { afterEach, describe, expect, it, vi } from "vitest";

import { emitSendEvent } from "@/features/compose/session/sendEvents";

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
});
