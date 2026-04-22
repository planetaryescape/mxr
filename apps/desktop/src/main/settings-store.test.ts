import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DesktopSettingsStore, DEFAULT_DESKTOP_SETTINGS } from "./settings-store.js";

describe("DesktopSettingsStore", () => {
  let cwd: string;

  beforeEach(async () => {
    cwd = await mkdtemp(join(tmpdir(), "mxr-desktop-settings-"));
  });

  afterEach(async () => {
    await rm(cwd, { force: true, recursive: true });
  });

  it("returns default settings when no settings file exists", () => {
    const store = new DesktopSettingsStore({
      cwd,
      name: "desktop-settings-test",
    });

    expect(store.get()).toEqual(DEFAULT_DESKTOP_SETTINGS);
  });

  it("persists theme and keymap overrides", () => {
    const store = new DesktopSettingsStore({
      cwd,
      name: "desktop-settings-test",
    });

    const saved = store.set({
      theme: "catppuccin-mocha",
      keymapOverrides: {
        rules: {
          n: "open_rule_form_new",
        },
      },
    });

    expect(saved).toMatchObject({
      theme: "catppuccin-mocha",
      keymapOverrides: {
        rules: {
          n: "open_rule_form_new",
        },
      },
    });

    const reloaded = new DesktopSettingsStore({
      cwd,
      name: "desktop-settings-test",
    });

    expect(reloaded.get()).toMatchObject({
      theme: "catppuccin-mocha",
      keymapOverrides: {
        rules: {
          n: "open_rule_form_new",
        },
      },
    });
  });
});
