import { act, render } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

  it("sits top centre with rich colours", async () => {
    const { container, findByText } = render(<Toaster />);
    act(() => {
      toast.info("Archived 1 message");
    });
    await findByText("Archived 1 message");
    const shown = container.querySelector("[data-sonner-toast]");
    expect(shown?.getAttribute("data-y-position")).toBe("top");
    expect(shown?.getAttribute("data-x-position")).toBe("center");
    expect(shown?.getAttribute("data-rich-colors")).toBe("true");
    expect(shown?.getAttribute("data-type")).toBe("info");
  });
});
