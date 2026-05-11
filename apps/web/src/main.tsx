import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "@/App";
import { bootstrapFromHash } from "@/lib/tokenStorage";
import { applyDensityAttribute, applyThemeAttribute, useUiPrefs } from "@/state/uiPrefsStore";
import "@/styles/app.css";

// Bootstrap auth + UI prefs synchronously so we don't flash unstyled UI.
bootstrapFromHash();
const prefs = useUiPrefs.getState();
applyThemeAttribute(prefs.theme);
applyDensityAttribute(prefs.density);

useUiPrefs.subscribe((state, prev) => {
  if (state.theme !== prev.theme) applyThemeAttribute(state.theme);
  if (state.density !== prev.density) applyDensityAttribute(state.density);
});

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
