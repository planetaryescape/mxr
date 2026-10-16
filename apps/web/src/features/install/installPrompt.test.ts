import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  promptInstall,
  readDismissed,
  shouldOfferInstall,
  useInstallPrompt,
  writeDismissed,
  type BeforeInstallPromptEvent,
} from "./installPrompt";

function fakeStorage(overrides: Partial<Storage> = {}): Storage {
  const data = new Map<string, string>();
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
    removeItem: (key: string) => void data.delete(key),
    clear: () => data.clear(),
    key: () => null,
    length: 0,
    ...overrides,
  } as Storage;
}

describe("shouldOfferInstall", () => {
  test("offers only when installable, outside an installed app, and not dismissed", () => {
    expect(shouldOfferInstall({ installable: true, standalone: false, dismissed: false })).toBe(
      true,
    );
    expect(shouldOfferInstall({ installable: false, standalone: false, dismissed: false })).toBe(
      false,
    );
    expect(shouldOfferInstall({ installable: true, standalone: true, dismissed: false })).toBe(
      false,
    );
    expect(shouldOfferInstall({ installable: true, standalone: false, dismissed: true })).toBe(
      false,
    );
  });
});

describe("dismissal", () => {
  test("remembers a dismissal across reads", () => {
    const storage = fakeStorage();
    expect(readDismissed(storage)).toBe(false);
    writeDismissed(storage);
    expect(readDismissed(storage)).toBe(true);
  });

  test("a throwing storage reads as not dismissed and never throws on write", () => {
    const broken = fakeStorage({
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(readDismissed(broken)).toBe(false);
    expect(() => writeDismissed(broken)).not.toThrow();
    expect(readDismissed(null)).toBe(false);
  });
});

describe("promptInstall", () => {
  beforeEach(() => useInstallPrompt.setState({ deferred: null }));

  test("does nothing without a held prompt", async () => {
    expect(await promptInstall()).toBe(false);
  });

  test("uses the held prompt once and reports acceptance", async () => {
    const prompt = vi.fn<() => Promise<void>>(() => Promise.resolve());
    const event = {
      prompt,
      userChoice: Promise.resolve({ outcome: "accepted" as const }),
    } as unknown as BeforeInstallPromptEvent;
    useInstallPrompt.setState({ deferred: event });

    expect(await promptInstall()).toBe(true);
    expect(prompt).toHaveBeenCalledTimes(1);
    expect(useInstallPrompt.getState().deferred).toBeNull();
    expect(await promptInstall()).toBe(false);
  });
});
