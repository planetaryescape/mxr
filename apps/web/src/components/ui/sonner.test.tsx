import { act, render } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { runLatestUndo, useUndo } from "@/state/undoStore";
import { Toaster } from "./sonner";

const assertive = (container: HTMLElement) =>
  container.querySelector<HTMLElement>('[aria-live="assertive"]');

// Sonner reads the OS scheme for its theme; jsdom has no matchMedia.
beforeEach(() => {
  vi.stubGlobal(
    "matchMedia",
    (
      query: string,
    ): Pick<MediaQueryList, "matches" | "media" | "addEventListener" | "removeEventListener"> & {
      addListener: () => void;
      removeListener: () => void;
    } => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
  act(() => {
    toast.dismiss();
  });
  vi.useRealTimers();
  useUndo.getState().clear();
});

describe("Toaster", () => {
  it("says an error again in the assertive region, and nothing else there", async () => {
    const { container, findByText } = render(<Toaster />);
    act(() => {
      toast.success("Done here");
    });
    await findByText("Done here");
    expect(assertive(container)?.textContent).toBe("");

    act(() => {
      toast.error("Archive failed");
    });
    await findByText("Archive failed", { selector: "[data-title]" });
    expect(assertive(container)?.textContent).toBe("Archive failed");
  });

  it("sits top right with rich colours", async () => {
    const { container, findByText } = render(<Toaster />);
    act(() => {
      toast.info("Archived 1 message");
    });
    await findByText("Archived 1 message");
    const shown = container.querySelector("[data-sonner-toast]");
    expect(shown?.getAttribute("data-y-position")).toBe("top");
    expect(shown?.getAttribute("data-x-position")).toBe("right");
    expect(shown?.getAttribute("data-rich-colors")).toBe("true");
    expect(shown?.getAttribute("data-type")).toBe("info");
  });

  it("pauses for keyboard focus, then leaves undo available after dismissal", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { container, findByText } = render(<Toaster />);
    const undo = vi.fn<() => Promise<boolean>>(async () => true);
    useUndo.getState().recordUndo(undo);

    act(() => {
      toast.success("Archived", {
        duration: 1_000,
        action: { label: "Undo", onClick: () => void undo() },
      });
    });
    await findByText("Archived");
    const shown = container.querySelector<HTMLElement>("[data-sonner-toast]");
    const button = container.querySelector<HTMLButtonElement>("[data-button]");
    expect(shown).not.toBeNull();
    expect(button).not.toBeNull();

    act(() => button?.focus());
    act(() => vi.advanceTimersByTime(1_100));
    expect(shown?.getAttribute("data-removed")).not.toBe("true");

    act(() => button?.blur());
    act(() => vi.advanceTimersByTime(1_000));
    expect(shown?.getAttribute("data-removed")).toBe("true");
    expect(useUndo.getState().lastUndo).toBe(undo);
    expect(runLatestUndo()).toBe("mail");
    expect(undo).toHaveBeenCalledOnce();
  });
});
