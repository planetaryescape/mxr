/*
 * "Install as app" in the palette, present only while the browser can
 * install mxr. The offer itself lives in the status bar.
 */

import { Download } from "lucide-react";

import { promptInstall, useInstallPrompt } from "@/features/install/installPrompt";
import { useModals } from "@/state/modalStore";

import type { ActionBehavior } from "./types";

export const installActions: ActionBehavior[] = [
  {
    id: "shell.install-app",
    icon: Download,
    when: () => useInstallPrompt.getState().deferred !== null,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      void promptInstall();
    },
  },
];
