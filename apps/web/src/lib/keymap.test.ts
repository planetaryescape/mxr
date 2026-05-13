/* @vitest-environment jsdom */

import { describe, expect, test, vi } from "vitest";

import { buildGlobalKeymap } from "./keymap";
import { useModals } from "@/state/modalStore";

describe("buildGlobalKeymap", () => {
  test("registers the registry-derived chords for nav.inbox + its aliases", () => {
    const nav = { navigate: vi.fn<(to: string) => void>() };
    const map = buildGlobalKeymap(nav);

    expect(typeof map["g i"]).toBe("function");
    expect(typeof map["1"]).toBe("function");
    expect(typeof map.Digit1).toBe("function");
  });

  test("g a binds to archive (not analytics) after the migration", () => {
    const nav = { navigate: vi.fn<(to: string) => void>() };
    const map = buildGlobalKeymap(nav);

    map["g a"]?.(new KeyboardEvent("keydown"));
    expect(nav.navigate).toHaveBeenCalledWith("/m/archive");
  });

  test("g y opens analytics", () => {
    const nav = { navigate: vi.fn<(to: string) => void>() };
    const map = buildGlobalKeymap(nav);

    map["g y"]?.(new KeyboardEvent("keydown"));
    expect(nav.navigate).toHaveBeenCalledWith("/analytics");
  });

  test("Shift+Semicolon opens the command palette", () => {
    const nav = { navigate: vi.fn<(to: string) => void>() };
    const map = buildGlobalKeymap(nav);
    useModals.setState({ commandPaletteOpen: false });

    map["Shift+Semicolon"]?.(new KeyboardEvent("keydown"));
    expect(useModals.getState().commandPaletteOpen).toBe(true);
  });

  test("chord handlers no-op when typing in an input field", () => {
    const nav = { navigate: vi.fn<(to: string) => void>() };
    const map = buildGlobalKeymap(nav);
    const input = document.createElement("input");
    document.body.appendChild(input);
    const event = new KeyboardEvent("keydown");
    Object.defineProperty(event, "target", { value: input });

    map["g i"]?.(event);
    expect(nav.navigate).not.toHaveBeenCalled();
    input.remove();
  });
});
