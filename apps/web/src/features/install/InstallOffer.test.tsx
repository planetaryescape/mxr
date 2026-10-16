/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import { InstallOffer } from "./InstallOffer";
import { useInstallPrompt, type BeforeInstallPromptEvent } from "./installPrompt";

const DISMISSED_KEY = "mxr.installOffer.dismissed";

function fakeEvent(): BeforeInstallPromptEvent {
  return {
    prompt: () => Promise.resolve(),
    userChoice: Promise.resolve({ outcome: "dismissed" as const }),
  } as unknown as BeforeInstallPromptEvent;
}

describe("InstallOffer", () => {
  beforeEach(() => {
    localStorage.removeItem(DISMISSED_KEY);
    useInstallPrompt.setState({ deferred: null });
  });
  afterEach(() => cleanup());

  test("stays hidden until the browser has offered an install", () => {
    render(<InstallOffer />);
    expect(screen.queryByTestId("install-offer")).toBeNull();
  });

  test("shows once the install prompt is held, and hides when dismissed", () => {
    useInstallPrompt.setState({ deferred: fakeEvent() });
    const { unmount } = render(<InstallOffer />);
    expect(screen.getByRole("button", { name: "Install mxr as an app" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Hide the install offer" }));
    expect(screen.queryByTestId("install-offer")).toBeNull();
    expect(localStorage.getItem(DISMISSED_KEY)).toBe("1");

    unmount();
    render(<InstallOffer />);
    expect(screen.queryByTestId("install-offer")).toBeNull();
  });
});
