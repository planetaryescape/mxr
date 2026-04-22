import { describe, expect, it, vi } from "vitest";
import type { DesktopActionContext } from "./desktop-actions";
import { runDesktopAction } from "./desktop-actions";

function createDiagnosticsContext() {
  const switchScreen = vi.fn();
  const setDiagnosticsSection = vi.fn();
  const setFocusContext = vi.fn();

  return {
    switchScreen,
    setDiagnosticsSection,
    setFocusContext,
    context: {
      switchScreen,
      setDiagnosticsSection,
      setFocusContext,
    } as unknown as DesktopActionContext,
  };
}

describe("runDesktopAction", () => {
  it("opens a diagnostics section with the shared diagnostics action", () => {
    const { context, switchScreen, setDiagnosticsSection, setFocusContext } =
      createDiagnosticsContext();

    runDesktopAction("open_diagnostics_section:labels", context);

    expect(switchScreen).toHaveBeenCalledWith("diagnostics");
    expect(setDiagnosticsSection).toHaveBeenCalledWith("labels");
    expect(setFocusContext).toHaveBeenCalledWith("mailList");
  });
});
