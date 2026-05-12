import Store, { type Options as StoreOptions } from "electron-store";
import type { DesktopSettings, DesktopSettingsPatch } from "../shared/types.js";

export const DEFAULT_DESKTOP_SETTINGS: DesktopSettings = {
  theme: "mxr-dark",
  keymapOverrides: {},
  telemetry: {
    sentryEnabled: false,
  },
};

export class DesktopSettingsStore {
  private readonly store: Store<DesktopSettings>;

  constructor(options?: StoreOptions<DesktopSettings>) {
    this.store = new Store<DesktopSettings>({
      name: "desktop-settings",
      defaults: DEFAULT_DESKTOP_SETTINGS,
      ...options,
    });
  }

  get(): DesktopSettings {
    return {
      theme: this.store.get("theme") ?? DEFAULT_DESKTOP_SETTINGS.theme,
      keymapOverrides:
        this.store.get("keymapOverrides") ?? DEFAULT_DESKTOP_SETTINGS.keymapOverrides,
      telemetry: {
        ...DEFAULT_DESKTOP_SETTINGS.telemetry,
        ...this.store.get("telemetry"),
      },
    };
  }

  set(patch: DesktopSettingsPatch): DesktopSettings {
    const current = this.get();
    const next: DesktopSettings = {
      ...current,
      ...patch,
      keymapOverrides: patch.keymapOverrides ?? current.keymapOverrides,
      telemetry: patch.telemetry ? { ...current.telemetry, ...patch.telemetry } : current.telemetry,
    };

    this.store.set(next);
    return this.get();
  }
}
