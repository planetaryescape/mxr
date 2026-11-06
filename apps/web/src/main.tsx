import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { registerSW } from "virtual:pwa-register";
import { toast } from "sonner";

import App from "@/App";
import { listenForInstallPrompt } from "@/features/install/installPrompt";
import { loadServedKeymap } from "@/lib/actions/servedKeymap";
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
    // The first page may have loaded before its worker finished installing.
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

// The person's keys (keys.toml) before the first render, so no view shows a
// default key that is about to change.
void loadServedKeymap().finally(() => {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
});
