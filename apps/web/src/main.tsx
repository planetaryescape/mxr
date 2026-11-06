/*
 * The entry module. Two ordering rules matter here:
 *
 * 1. `servedKeymap` is imported first. Its module body puts the last keymap
 *    the daemon served into the registry, and modules that build their key
 *    labels once (navMap, sidebarRail) are evaluated while `@/App` loads,
 *    before this file's body runs. A later import order would paint the
 *    definition's keys and then leave them there.
 * 2. Nothing waits for a network answer before the first render. Auth, UI
 *    prefs and the keymap cache are synchronous local reads; the keymap
 *    request runs in the background and re-renders what reads it.
 */
import { loadServedKeymap } from "@/lib/actions/servedKeymap";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { registerSW } from "virtual:pwa-register";

import App from "@/App";
import { listenForInstallPrompt } from "@/features/install/installPrompt";
import { bootstrapFromHash } from "@/lib/tokenStorage";
import {
  applyDensityAttribute,
  applyContrastAttribute,
  applyMotionAttribute,
  applyThemeAttribute,
  useUiPrefs,
  watchSystemMotion,
  watchSystemTheme,
  watchSystemContrast,
} from "@/state/uiPrefsStore";
import "@/styles/app.css";

// Bootstrap auth + UI prefs synchronously so we don't flash unstyled UI.
bootstrapFromHash();
const prefs = useUiPrefs.getState();
applyThemeAttribute(prefs.theme);
applyDensityAttribute(prefs.density);
applyMotionAttribute(prefs.motion);
applyContrastAttribute(prefs.contrast);
watchSystemTheme();
watchSystemMotion();
watchSystemContrast();

useUiPrefs.subscribe((state, prev) => {
  if (state.theme !== prev.theme) applyThemeAttribute(state.theme);
  if (state.density !== prev.density) applyDensityAttribute(state.density);
  if (state.motion !== prev.motion) applyMotionAttribute(state.motion);
  if (state.contrast !== prev.contrast) applyContrastAttribute(state.contrast);
});

registerSW({ immediate: true });
listenForInstallPrompt();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The person's keys (keys.toml) arrive after the first paint; the store's
// version bumps, so hints, the palette and the dispatcher take the new keys
// without a reload. The refresh starts once the first screen has painted, so
// its 184 kB answer can't take bandwidth from the chunks and queries the
// first screen needs. A browser that never reports the paint still asks.
function refreshKeys(): void {
  let started = false;
  const start = () => {
    if (started) return;
    started = true;
    if (typeof requestIdleCallback === "function") {
      requestIdleCallback(() => void loadServedKeymap(), { timeout: 1000 });
    } else {
      void loadServedKeymap();
    }
  };
  try {
    const painted = new PerformanceObserver((list) => {
      if (!list.getEntries().some((entry) => entry.name === "first-contentful-paint")) return;
      painted.disconnect();
      start();
    });
    painted.observe({ type: "paint", buffered: true });
    setTimeout(() => {
      painted.disconnect();
      start();
    }, 4000);
  } catch {
    start();
  }
}

refreshKeys();
