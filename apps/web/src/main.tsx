import { loadServedKeymap } from "@/lib/actions/servedKeymap";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { registerSW } from "virtual:pwa-register";
import { toast } from "sonner";

import App from "@/App";
import { listenForInstallPrompt } from "@/features/install/installPrompt";
import { bootstrapFromHash } from "@/lib/tokenStorage";
import { getActiveQueryClient } from "@/lib/queryClient";
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

const updateApp = registerSW({
  immediate: true,
  onNeedRefresh() {
    toast("An mxr update is ready", {
      description: "Reload when you have finished editing.",
      duration: Infinity,
      action: { label: "Update", onClick: () => void updateApp(true) },
    });
  },
});
if ("serviceWorker" in navigator) {
  let controlled = Boolean(navigator.serviceWorker.controller);
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (controlled) return;
    controlled = true;
    void getActiveQueryClient()?.refetchQueries({
      type: "active",
      predicate: (query) => query.meta?.keepThroughGaps !== true,
    });
  });
  navigator.serviceWorker.addEventListener("message", (event) => {
    const target = "/settings/token?reason=expired";
    if (
      event.data?.type === "MXR_SESSION_REJECTED" &&
      window.location.pathname + window.location.search !== target
    ) {
      window.location.assign(target);
    }
  });
}
listenForInstallPrompt();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

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
