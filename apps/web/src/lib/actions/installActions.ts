/*
 * "Install as app" in the palette, present only while the browser can
 * install mxr. The offer itself lives in the status bar.
 */

import { Download } from "lucide-react";

import { promptInstall, useInstallPrompt } from "@/features/install/installPrompt";
import { useModals } from "@/state/modalStore";

import type { Action } from "./types";

export const installActions: Action[] = [
  {
    id: "shell.install-app",
    label: "Install as app",
    description: "Open mxr in its own window, with its own keys",
    group: "Settings",
    icon: Download,
    paletteOnly: true,
    when: () => useInstallPrompt.getState().deferred !== null,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      void promptInstall();
    },
  },
];
